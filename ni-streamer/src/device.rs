//! Implements the [`StreamableDevice`] trait for [`nicompiler_backend::Device`] to support streaming
//! operations on NI hardware.
//!
//! This module serves as a bridge between the backend logic and the NI-DAQ hardware, ensuring seamless
//! streaming operations and synchronized behavior across devices. It implements the specifications of
//! [`nicompiler_backend::device`] to translates compiled instructions within a device into NI-DAQmx driver
//! instructions, while ensuring synchronized and efficient streaming.
//!
//! ## Overview:
//!
//! The [`StreamableDevice`] trait extends the [`nicompiler_backend::BaseDevice`] trait,
//! adding [`StreamableDevice::stream_task`] that allow for the streaming of instruction signals
//! onto specified NI-DAQ devices.
//!
//! ## Key Components:
//!
//! - [`StreamableDevice`] Trait: The primary trait that encapsulates the extended functionality. It defines methods
//!   to stream signals, configure task channels, and set up synchronization and clocking.
//! - Helper Methods: Helper methods like `cfg_task_channels` and `cfg_clk_sync` within the trait.
//!   simplify the device configuration process.
//!
//! ## Features:
//!
//! - **Streaming**: The primary feature, allowing for the streaming of instruction signals to NI-DAQ devices.
//! - **Synchronization**: Ensures that multiple devices can operate in a synchronized manner, especially
//!   crucial when there's a primary and secondary device setup.
//! - **Clock Configuration**: Sets up the sample clock, start trigger, and reference clocking for devices.
//! - **Task Channel Configuration**: Configures the task channels based on the device's task type.
use std::cmp::Ordering;
use std::collections::BTreeSet;
use std::iter::zip;
use std::fmt::{Debug, Formatter};
use std::sync::mpsc::{Sender, Receiver, SendError, RecvError};

use indexmap::IndexMap;
use itertools::Itertools;

use base_streamer::channel::BaseChan;
use base_streamer::device::BaseDev;
use base_streamer::streamer::TypedDev;

use crate::channel::{AOChan, DOChan, DOPort};
use crate::nidaqmx::*;
use crate::utils::StreamCounter;
use crate::worker_cmd_chan::{CmdRecvr, WorkerCmd};

pub enum StartSync {
    Primary(Vec<Receiver<()>>),
    Secondary(Sender<()>),
    None
}

pub struct WorkerError {
    msg: String
}
impl From<SendError<()>> for WorkerError {
    fn from(_value: SendError<()>) -> Self {
        Self {
            msg: "Worker thread encountered SendError".to_string()
        }
    }
}
impl From<RecvError> for WorkerError {
    fn from(_value: RecvError) -> Self {
        Self {
            msg: "Worker encountered RecvError".to_string()
        }
    }
}
impl From<DAQmxError> for WorkerError {
    fn from(value: DAQmxError) -> Self {
        Self{msg: value.to_string()}
    }
}
impl From<String> for WorkerError {
    fn from(value: String) -> Self {
        Self {
            msg: format!("Worker thread encountered the following error: \n{value}")
        }
    }
}
impl ToString for WorkerError {
    fn to_string(&self) -> String {
        self.msg.clone()
    }
}

pub struct StreamBundle {
    ni_task: NiTask,
    counter: StreamCounter,
    buf_write_timeout: Option<f64>,  // Some(finite_timeout_in_seconds) or None - wait infinitely
}

pub enum SampBufs {
    AO(Vec<f64>),
    DOLinesPorts((Vec<bool>, Vec<u32>)),
    DOPorts(Vec<u32>)
}
impl Debug for SampBufs {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let variant_str = match self {
            SampBufs::AO(_) => "AO".to_string(),
            SampBufs::DOLinesPorts(_) => "DOLinesPorts".to_string(),
            SampBufs::DOPorts(_) => "DOPorts".to_string(),
        };
        write!(f, "{}", variant_str)
    }
}

pub struct HwCfg {
    pub start_trig_in: Option<String>,
    pub start_trig_out: Option<String>,
    pub samp_clk_in: Option<String>,
    pub samp_clk_out: Option<String>,
    pub ref_clk_in: Option<String>,
    pub min_bufwrite_timeout: Option<f64>,  // Some(finite_timeout_in_seconds) or None - wait infinitely
}
impl HwCfg {
    pub fn dflt() -> Self {
        Self {
            start_trig_in: None,
            start_trig_out: None,
            samp_clk_in: None,
            samp_clk_out: None,
            ref_clk_in: None,
            min_bufwrite_timeout: Some(5.0),
        }
    }
}

pub trait CommonHwCfg {
    fn hw_cfg(&self) -> &HwCfg;
    fn hw_cfg_mut(&mut self) -> &mut HwCfg;
}

/// The `StreamableDevice` trait extends the [`nicompiler_backend::BaseDevice`] trait of [`nicompiler_backend::Device`]
/// to provide additional functionality for streaming tasks.
pub trait StreamDev<T, C>: BaseDev<T, C> + CommonHwCfg + Sync + Send
where
    T: Clone + Debug + Send + Sync + 'static,  // output sample data type
    C: BaseChan<T>  // channel type
{
    /// Helper function that configures the task channels for the device.
    ///
    /// This method is a helper utility designed to configure the task channels based on the device's `task_type`.
    /// It invokes the corresponding DAQmx driver method to set up the channels, ensuring they are correctly initialized
    /// for subsequent operations. This method is invoked by [`StreamableDevice::stream_task`].
    ///
    /// # Parameters
    ///
    /// * `task`: A reference to the `NiTask` instance representing the task to be configured.
    ///
    /// # Behavior
    ///
    /// Depending on the device's `task_type`, the method will:
    /// * For `TaskType::AO`: Iterate through the compiled, streamable channels and invoke the
    /// `create_ao_chan` method for each channel.
    /// * For `TaskType::DO`: Iterate through the compiled, streamable channels and invoke the
    /// `create_do_chan` method for each channel.
    ///
    /// The channel names are constructed using the format `/{device_name}/{channel_name}`.
    // fn total_samp_num(&self) -> usize;

    fn create_task_chans(&self, task: &NiTask) -> Result<(), DAQmxError>;
    fn alloc_samp_bufs(&self, buf_size: usize) -> SampBufs;
    fn calc_samps_(&self, samp_bufs: &mut SampBufs, start_pos: usize, end_pos: usize) -> Result<(), String>;
    fn write_to_hardware(&self, bundle: &mut StreamBundle, bufs: &SampBufs, samp_num: usize) -> Result<usize, DAQmxError>;
    //      to remove dependency on `trait BaseDev<T>` and remove type parameter `T`:
    // fn total_samps(self) -> usize;

    /// Streams an instruction signal to the specified NI-DAQ device.
    ///
    /// This method is responsible for streaming an instruction signal to a National Instruments (NI) DAQ device
    /// represented by `self`. It sets up a new NI-DAQmx task, configures synchronization methods and buffer,
    /// writes the initial chunk of the sequence into the driver buffer, and starts the task, causing the device
    /// to output the signal.
    ///
    /// # Parameters
    ///
    /// * `sem`: A semaphore used to synchronize the start triggers between multiple devices. Ensures that threads
    ///   for secondary devices always start listening for triggers before the primary device starts and exports
    ///   its start trigger.
    /// * `num_devices`: The total number of NI-DAQ devices involved in the streaming process.
    /// * `stream_buftime`: Duration (in milliseconds) specifying the length of the streaming buffer.
    /// * `nreps`: Number of repetitions for streaming the sequence. Streaming a sequence multiple times in a single
    ///   call using `nreps` is more efficient than multiple separate calls.
    ///
    /// # Behavior
    ///
    /// 1. Asserts that the device has been compiled using `is_compiled`.
    /// 2. Initializes a new `NiTask` and configures the device channels.
    /// 3. Configures the buffer, writing method, clock, and synchronization.
    /// 4. Writes the initial chunk of the sequence into the driver buffer.
    /// 5. Starts the task, causing the device to output the signal.
    /// 6. Continuously streams chunks of the sequence to the device until the entire sequence has been streamed.
    /// 7. If `nreps` > 1, the sequence is streamed the specified number of times.
    ///
    /// The method uses a `TickTimer` to measure the time taken for various operations, which can be helpful for
    /// performance analysis.
    ///
    /// # Safety and Synchronization
    ///
    /// The method uses a semaphore (`sem`) to ensure synchronization between multiple devices. Specifically, it ensures
    /// that secondary devices start listening for triggers before the primary device starts and exports its start trigger.
    ///
    /// # Note
    ///
    /// The method relies on various helper functions and methods, such as `is_compiled`, `cfg_task_channels`, and
    /// `calc_signal_nsamps`, to achieve its functionality. Ensure that all dependencies are correctly set up and
    /// that the device has been properly compiled before calling this method.
    fn worker_loop(
        &mut self,
        bufsize_ms: f64,
        mut cmd_recvr: CmdRecvr,
        report_sendr: Sender<()>,
        start_sync: StartSync,
    ) -> Result<(), WorkerError> {
        let (mut stream_bundle, mut samp_bufs) = self.cfg_run_(bufsize_ms)?;
        report_sendr.send(())?;

        loop {
            match cmd_recvr.recv()? {
                WorkerCmd::Stream(calc_next) => {
                    self.stream_run_(&mut stream_bundle, &mut samp_bufs, &start_sync, calc_next)?;
                    report_sendr.send(())?;
                },
                WorkerCmd::Close => {
                    break
                }
            }
        };
        Ok(())
    }
    fn cfg_run_(&self, bufsize_ms: f64) -> Result<(StreamBundle, SampBufs), WorkerError> {
        let buf_dur = bufsize_ms / 1000.0;
        let buf_write_timeout = match &self.hw_cfg().min_bufwrite_timeout {
            Some(min_timeout) => Some(f64::max(10.0*buf_dur, *min_timeout)),
            None => None,
        };

        let seq_len = self.total_samps();
        let buf_size = std::cmp::min(
            seq_len,
            (buf_dur * self.samp_rate()).round() as usize,
        );
        let mut samp_buf = self.alloc_samp_bufs(buf_size);
        let counter = StreamCounter::new(seq_len, buf_size);

        // DAQmx Setup
        let task = NiTask::new()?;
        self.create_task_chans(&task)?;
        task.cfg_output_buf(buf_size)?;
        task.disallow_regen()?;
        self.cfg_clk_sync(&task, seq_len)?;

        // Bundle NiTask, StreamCounter, and buf_write_timeout together for convenience:
        let mut stream_bundle = StreamBundle {
            ni_task: task,
            counter,
            buf_write_timeout,
        };

        // Calc and write the initial sample chunk into the buffer
        let (start_pos, end_pos) = stream_bundle.counter.tick_next().unwrap();
        self.calc_samps_(&mut samp_buf, start_pos, end_pos)?;
        self.write_to_hardware(&mut stream_bundle, &samp_buf, end_pos - start_pos)?;

        Ok((stream_bundle, samp_buf))
    }
    fn stream_run_(
        &self,
        stream_bundle: &mut StreamBundle,
        samp_bufs: &mut SampBufs,
        start_sync: &StartSync,
        calc_next: bool
    ) -> Result<(), WorkerError> {
        // Synchronise task start with other threads
        match start_sync {
            StartSync::Primary(recvr_vec) => {
                for recvr in recvr_vec {
                    recvr.recv()?
                };
                stream_bundle.ni_task.start()?;
            },
            StartSync::Secondary(sender) => {
                stream_bundle.ni_task.start()?;
                sender.send(())?;
            },
            StartSync::None => stream_bundle.ni_task.start()?
        };

        // Main streaming loop
        while let Some((start_pos, end_pos)) = stream_bundle.counter.tick_next() {
            self.calc_samps_(samp_bufs, start_pos, end_pos)?;
            self.write_to_hardware(stream_bundle, samp_bufs, end_pos - start_pos)?;
        }

        // Now need to wait for the final sample chunk to be generated out by the card before stopping the task.
        // In the mean time, we can calculate the initial chunk for the next repetition in the case we are on repeat.
        if !calc_next {
            stream_bundle.ni_task.wait_until_done(stream_bundle.buf_write_timeout.clone())?;
            stream_bundle.ni_task.stop()?;
        } else {
            stream_bundle.counter.reset();
            let (start_pos, end_pos) = stream_bundle.counter.tick_next().unwrap();
            self.calc_samps_(samp_bufs, start_pos, end_pos)?;

            stream_bundle.ni_task.wait_until_done(stream_bundle.buf_write_timeout.clone())?;
            stream_bundle.ni_task.stop()?;

            self.write_to_hardware(stream_bundle, samp_bufs, end_pos - start_pos)?;
        }
        Ok(())
    }

    /// Configures the synchronization and clock behavior for the device.
    ///
    /// This method sets up the synchronization behavior of the device, ensuring that its operation is correctly
    /// coordinated with other devices or tasks. It configures the sample clock, start trigger, and reference clocking.
    /// This method is invoked by [`StreamableDevice::stream_task`].
    ///
    /// Refer to [`nicompiler_backend::Device`] for a detailed explanation of synchronization mechanisms and their importance.
    ///
    /// # Parameters
    ///
    /// * `task`: A reference to the `NiTask` instance representing the task to be synchronized.
    /// * `seq_len`: A reference to the length of the sequence for which synchronization is required.
    ///
    /// # Behavior
    ///
    /// 1. Configures the sample clock using the provided `samp_clk_src` and `samp_rate`.
    /// 2. If the device has a trigger line, it configures the start trigger. Primary devices will export the start trigger,
    ///    while secondary devices will configure their tasks to expect the start trigger.
    /// 3. Configures reference clocking based on the device's `ref_clk_line`. Devices that import the reference clock will
    ///    configure it accordingly, while others will export the signal.
    fn cfg_clk_sync(&self, task: &NiTask, seq_len: usize) -> Result<(), DAQmxError> {
        // (1) Sample clock timing mode (includes sample clock source). Additionally, config samp_clk_out
        let samp_clk_src = self.hw_cfg().samp_clk_in.clone().unwrap_or("".to_string());
        task.cfg_samp_clk_timing(
            &samp_clk_src,
            self.samp_rate(),
            seq_len as u64
        )?;
        if let Some(term) = &self.hw_cfg().samp_clk_out {
            task.export_signal(
                DAQMX_VAL_SAMPLECLOCK,
                &format!("/{}/{}", self.name(), term)
            )?
        };

        // (2) Start trigger:
        if let Some(term) = &self.hw_cfg().start_trig_in {
            task.cfg_dig_edge_start_trigger(&format!("/{}/{}", self.name(), term))?
        };
        if let Some(term) = &self.hw_cfg().start_trig_out {
            task.export_signal(
                DAQMX_VAL_STARTTRIGGER,
                &format!("/{}/{}", self.name(), term)
            )?
        };

        // (3) Reference clock
        /*  Only handling ref_clk import here.

        The "easily accessible" static ref_clk export from a single card should have already been done
        by the Streamer if user specified `ref_clk_provider`.
        Not providing the "easy access" to exporting ref_clk from more than one card on purpose.

        (Reminder: we are using static ref_clk export (as opposed to task-based export) to be able to always use
        the same card as the clock reference source even if this card does not run this time)

        NIDAQmx allows exporting 10MHz ref_clk from more than one card. And this even has a realistic use case
        of chained clock locking when a given card both locks to external ref_clk and exports its own
        reference for use by another card.

        The risk is that the user may do ref_clk export and forget to add pulses to this card. In such case
        the reference signal will show up but it will not be locked to the input reference
        since locking is only done on the per-task basis. This may lead to very hard-to-find footguns
        because it is hard to distinguish between locked and free-running 10MHz signals.

        For that reason, we still leave room for arbitrary (static) export from any number of cards,
        but only expose it through the "advanced" function `nidaqmx::connect_terms()`.
        */
        if let Some(term) = &self.hw_cfg().ref_clk_in {
            task.set_ref_clk_src(&format!("/{}/{}", self.name(), term))?;
            task.set_ref_clk_rate(10.0e6)?;
        };

        Ok(())
    }
}

// region AO Device
pub struct AODev {
    name: String,
    samp_rate: f64,
    chans: IndexMap<String, AOChan>,
    hw_cfg: HwCfg,
}

impl AODev {
    pub fn new(name: &str, samp_rate: f64) -> Self {
        Self {
            name: name.to_string(),
            samp_rate,
            chans: IndexMap::new(),
            hw_cfg: HwCfg::dflt(),
        }
    }

    pub fn add_chan_sort(&mut self, chan: AOChan) -> Result<(), String> {
        self.add_chan(chan)?;
        self.chans.sort_by(
            |_k1, v1, _k2, v2| {
                v1.idx().cmp(&v2.idx())
            }
        );
        Ok(())
    }
}

impl BaseDev<f64, AOChan> for AODev {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
    }

    fn chans(&self) -> &IndexMap<String, AOChan> {
        &self.chans
    }

    fn chans_mut(&mut self) -> &mut IndexMap<String, AOChan> {
        &mut self.chans
    }
}

impl CommonHwCfg for AODev {
    fn hw_cfg(&self) -> &HwCfg {
        &self.hw_cfg
    }

    fn hw_cfg_mut(&mut self) -> &mut HwCfg {
        &mut self.hw_cfg
    }
}

impl StreamDev<f64, AOChan> for AODev {
    fn create_task_chans(&self, task: &NiTask) -> Result<(), DAQmxError> {
        for chan in self.compiled_chans().iter() {
            task.create_ao_chan(&format!("/{}/{}", self.name(), chan.name()))?;
        };
        Ok(())
    }

    fn alloc_samp_bufs(&self, buf_size: usize) -> SampBufs {
        SampBufs::AO(
            vec![0.0; buf_size * self.compiled_chans().len()]
        )
    }

    fn calc_samps_(&self, samp_bufs: &mut SampBufs, start_pos: usize, end_pos: usize) -> Result<(), String> {
        let samp_buf = match samp_bufs {
            SampBufs::AO(samp_buf) => samp_buf,
            other => return Err(format!("AODev::calc_samps_() received incorrect `SampBufs` variant {other:?}")),
        };
        BaseDev::calc_samps(self, &mut samp_buf[..], start_pos, end_pos)
    }

    fn write_to_hardware(&self, bundle: &mut StreamBundle, bufs: &SampBufs, samp_num: usize) -> Result<usize, DAQmxError> {
        let samp_buf = match bufs {
            SampBufs::AO(buf) => buf,
            other => return Err(DAQmxError::new(format!("AODev::write_to_hardware() received incorrect `SampBufs` variant {other:?}"))),
        };

        // Sanity check - requested `samp_num` is not too large
        if self.compiled_chans().len() * samp_num > samp_buf.len() {
            return Err(DAQmxError::new(format!(
                "[write_to_hardware()] BUG:\n\
                \tsamp_num * self.compiled_chans().len() = {} \n\
                exceeds the total number of samples available in the buffer\n\
                \tsamp_buf.len() = {}",
                self.compiled_chans().len() * samp_num,
                samp_buf.len()
            )))
        }

        // Write to hardware
        bundle.ni_task.write_analog(
            &samp_buf[..],
            samp_num,
            bundle.buf_write_timeout.clone()
        )
    }
}
// endregion

// region DO Device
pub struct DODev {
    name: String,
    samp_rate: f64,
    chans: IndexMap<String, DOChan>,
    hw_cfg: HwCfg,
    const_fns_only: bool,
    compiled_ports: Option<IndexMap<usize, DOPort>>
}

impl DODev {
    pub fn new(name: &str, samp_rate: f64) -> Self {
        Self {
            name: name.to_string(),
            samp_rate,
            chans: IndexMap::new(),
            hw_cfg: HwCfg::dflt(),
            const_fns_only: false,
            compiled_ports: None,
        }
    }

    pub fn add_chan_sort(&mut self, chan: DOChan) -> Result<(), String> {
        self.add_chan(chan)?;
        self.chans.sort_by(
            |_k1, v1, _k2, v2| {
                let (p1, l1) = (v1.port(), v1.line());
                let (p2, l2) = (v2.port(), v2.line());
                let port_cmp = p1.cmp(&p2);
                if port_cmp == Ordering::Equal {
                     l1.cmp(&l2)
                } else {
                    port_cmp
                }
            }
        );
        Ok(())
    }

    /// Note: there are _running_ ports no matter if `const_fns_only` mode is used or not.
    /// This function returns the numbers of the ports which will be added to NI task.
    ///
    /// * If `const_fn_only = true`, line->port merging was done during `BaseDev::compile()`
    ///   and port compile cache was stored in `DOPort` instances.
    ///
    /// * If `const_fn_only = false`, there are no "compiled" port instances after `BaseDev::compile()`
    ///   and line->port merging will have to be done sample-by-sample during `StreamDev::calc_samps_()`,
    ///   BUT there are still running ports and this function returns their numbers.
    pub fn running_port_nums(&self) -> Vec<usize> {
        let running_port_nums_ = self.compiled_chans()
            .iter()
            .map(|chan| chan.port())
            .unique()
            .sorted()
            .collect();

        // Consistency check if `const_fns_only` is used
        if self.const_fns_only {
            let compiled_port_nums: Vec<usize> = self.compiled_ports
                .as_ref()
                .unwrap()
                .keys()
                .map(|&num| num)
                .collect();
            assert_eq!(running_port_nums_, compiled_port_nums)
        }

        return running_port_nums_
    }
}

impl BaseDev<bool, DOChan> for DODev {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
    }

    fn chans(&self) -> &IndexMap<String, DOChan> {
        &self.chans
    }

    fn chans_mut(&mut self) -> &mut IndexMap<String, DOChan> {
        &mut self.chans
    }

    fn clear_compile_cache(&mut self) {
        for chan in self.chans_mut().values_mut() {
            chan.clear_compile_cache()
        }
        self.compiled_ports = None;
    }

    fn compile(&mut self, stop_time: f64) -> Result<f64, String> {
        self.clear_compile_cache();

        let total_run_time = BaseDev::compile_base(self, stop_time)?;
        if !self.const_fns_only {
            // Generic instruction case - cannot do line->port merging at compile time. Will have to merge samples on-the-fly during streaming.
            return Ok(total_run_time)
        }

        // Line -> port merging
        //      For "const-instruction only" case, do the line->port merging now, at compile time,
        //      to significantly reduce computational load during streaming.

        // (1) Group line channels by ports for convenience
        let mut port_map = IndexMap::new();
        for chan in self.compiled_chans() {
            let port_num = chan.port();
            let line_num = chan.line();

            if !port_map.contains_key(&port_num) {
                port_map.insert(port_num, IndexMap::new());
            }

            port_map.get_mut(&port_num).unwrap().insert(line_num, chan);
        }
        // Sort ports within the map and lines within each port
        port_map.sort_by(|k1, _v1, k2, _v2| k1.cmp(k2));
        for line_map in port_map.values_mut() {
            line_map.sort_by(|k1, _v1, k2, _v2| k1.cmp(k2))
        }

        // (2) Merge lines for each port
        let mut compiled_ports = IndexMap::new();
        for (&port_num, line_map) in port_map.iter() {
            // Collect instruction ends from all the lines of this port into one joint vector
            let mut port_instr_ends = BTreeSet::new();
            for chan in line_map.values() {
                port_instr_ends.extend(chan.compile_cache_ends())
            }
            let port_instr_ends: Vec<usize> = port_instr_ends.into_iter().collect();

            // Vector to store final instruction values for the port
            let mut port_instr_vals = vec![0_u32; port_instr_ends.len()];

            for (&line_num, chan) in line_map.iter() {
                let line_compile_cache = zip(
                    chan.compile_cache_ends(),
                    chan.compile_cache_fns()
                );
                let mut first_covered_idx = 0;
                for (&line_instr_end, line_instr_fn) in line_compile_cache {
                    // Extract the (constant) instruction value by evaluating the function object at some point
                    // The actual value of `t` should not matter - just make-up some one-element `t_arr` and `res_arr` to feed into `calc()`
                    let t_arr = [0.0_f64];
                    let mut res_arr = [false];
                    line_instr_fn.calc(&t_arr, &mut res_arr);
                    let const_val = res_arr[0];

                    // Find all the port instructions covered by this line instruction and add its' contribution
                    let last_covered_idx = port_instr_ends.binary_search(&line_instr_end).unwrap();

                    let line_contrib = (const_val as u32) << (line_num as u32);
                    for port_instr_val in &mut port_instr_vals[first_covered_idx..=last_covered_idx] {
                        *port_instr_val |= line_contrib;
                    }

                    first_covered_idx = last_covered_idx + 1;
                }
                assert_eq!(first_covered_idx, port_instr_ends.len());
            }

            // Create the port instance and store obtained compile cache
            let compiled_port = DOPort{
                idx: port_num,
                ends: port_instr_ends,
                vals: port_instr_vals
            };
            compiled_ports.insert(port_num, compiled_port);
        }

        // Finally, store all the compiled `DOPort`s in `DODev` compile cache field
        self.compiled_ports = Some(compiled_ports);

        Ok(total_run_time)
    }
}

impl CommonHwCfg for DODev {
    fn hw_cfg(&self) -> &HwCfg {
        &self.hw_cfg
    }

    fn hw_cfg_mut(&mut self) -> &mut HwCfg {
        &mut self.hw_cfg
    }
}

impl StreamDev<bool, DOChan> for DODev {
    fn create_task_chans(&self, task: &NiTask) -> Result<(), DAQmxError> {
        for port_num in self.running_port_nums() {
            task.create_do_chan(&format!("/{}/port{}", self.name(), port_num))?;
        }
        Ok(())
    }

    fn alloc_samp_bufs(&self, buf_size: usize) -> SampBufs {
        if self.const_fns_only {
            SampBufs::DOPorts(
                vec![0_u32; buf_size * self.running_port_nums().len()]
            )
        } else {
            SampBufs::DOLinesPorts((
                vec![false; buf_size * self.compiled_chans().len()],
                vec![0_u32; buf_size * self.running_port_nums().len()]
            ))
        }
    }

    fn calc_samps_(&self, samp_bufs: &mut SampBufs, start_pos: usize, end_pos: usize) -> Result<(), String> {
        if !(end_pos >= start_pos + 1) {
            return Err(format!(
                "StreamDev::calc_samps_() - requested start_pos={start_pos} and end_pos={end_pos} are invalid.\n\
                end_pos must be no less than start_pos + 1"
            ))
        }
        if end_pos > self.total_samps() {
            return Err(format!("StreamDev::calc_samps_() - requested end_pos={end_pos} exceeds total compiled sample number {}", self.total_samps()))
        }
        let samp_num = end_pos - start_pos;

        // (a) Case of constant-only functions.
        //  Line->port merging should have already been done during compilation.
        //  Now only need to calculate samples using already-compiled port channels
        if self.const_fns_only {
            // Extract the buffer
            let port_samp_buf = match samp_bufs {
                SampBufs::DOPorts(buf) => {
                    // Sanity check - the buffer is large enough
                    let port_samps_needed = self.running_port_nums().len() * samp_num;
                    if port_samps_needed > buf.len() {
                        return Err(format!("StreamDev::calc_samps_()::const_fns_only - ports sample number {port_samps_needed} exceeds buffer size {}", buf.len()))
                    };
                    buf
                },
                other => return Err(format!("DODev::calc_samps_()::const_fns_only received incorrect `SampBufs` variant {other:?}")),
            };

            for (port_row_idx, port) in self.compiled_ports.as_ref().unwrap().values().enumerate() {
                let port_slice = &mut port_samp_buf[port_row_idx * samp_num .. (port_row_idx + 1) * samp_num];
                port.fill_samps(start_pos, port_slice)?;
            }
        }

        // (b) Case of generic (not constant-only) functions
        //  No line->port merging was done during compiling.
        //  Need to first calculate sample vectors for each line individually
        //  and then merge them into port values sample-by-sample
        else {
            // Extract the buffers
            let (lines_buf, ports_buf) = match samp_bufs {
                SampBufs::DOLinesPorts((lines_buf, ports_buf)) => {
                    // Sanity checks - the buffers are large enough
                    let line_samps_needed = self.compiled_chans().len() * samp_num;
                    if line_samps_needed > lines_buf.len() {
                        return Err(format!("DODev::calc_samps_()::generic_fns - the lines sample number {line_samps_needed} exceeds line buffer size {}", lines_buf.len()))
                    }
                    let port_samps_needed = self.running_port_nums().len() * samp_num;
                    if port_samps_needed > ports_buf.len() {
                        return Err(format!("DODev::calc_samps_()::generic_fns - the ports sample number {port_samps_needed} exceeds port buffer size {}", ports_buf.len()))
                    }
                    (lines_buf, ports_buf)
                }
                other => return Err(format!("DODev::calc_samps_() received incorrect `SampBufs` variant {other:?}")),
            };

            // (1) Calculate samples for each line using `calc_samps()` from the `BaseDev` trait
            BaseDev::calc_samps(self, &mut lines_buf[..], start_pos, end_pos)?;

            // (2) Merge lines into ports sample-by-sample
            ports_buf.fill(0);  // clear previous values in the buffer

            for (port_row_idx, &port_num) in self.running_port_nums().iter().enumerate() {
                let port_slice = &mut ports_buf[port_row_idx * samp_num .. (port_row_idx + 1) * samp_num];

                for (chan_row_idx, chan) in self.compiled_chans().iter().enumerate() {
                    if chan.port() != port_num {
                        continue
                    }
                    let chan_slice = &lines_buf[chan_row_idx * samp_num .. (chan_row_idx + 1) * samp_num];
                    let line_num = chan.line();
                    for (port_samp, &line_samp) in port_slice.iter_mut().zip(chan_slice.iter()) {
                        *port_samp |= (line_samp as u32) << line_num;
                    }
                }
            }
        }

        Ok(())
    }

    fn write_to_hardware(&self, bundle: &mut StreamBundle, bufs: &SampBufs, samp_num: usize) -> Result<usize, DAQmxError> {
        let ports_buf = match bufs {
            SampBufs::DOPorts(buf) => buf,
            SampBufs::DOLinesPorts((_lines_buf, ports_buf)) => ports_buf,
            other => return Err(DAQmxError::new(format!("DODev::write_to_card() received incorrect `SampBufs` variant {other:?}")))
        };

        // Sanity check - requested `samp_num` is not too large
        if self.running_port_nums().len() * samp_num > ports_buf.len() {
            return Err(DAQmxError::new(format!(
                "[write_to_hardware()] BUG:\n\
                \tsamp_num * self.running_port_nums().len() = {} \n\
                exceeds the total number of samples available in the buffer\n\
                \tsamp_buf.len() = {}",
                samp_num * self.running_port_nums().len(),
                ports_buf.len()
            )))
        }

        // Write to hardware
        let samps_written = bundle.ni_task.write_digital_port(
            &ports_buf[..],
            samp_num,
            bundle.buf_write_timeout.clone()
        )?;
        Ok(samps_written)
    }
}
// endregion

// region NIDev (to treat AO and DO uniformly)
pub type NIDev = TypedDev<AODev, DODev>;

impl CommonHwCfg for NIDev {
    fn hw_cfg(&self) -> &HwCfg {
        match self {
            Self::AO(dev) => dev.hw_cfg(),
            Self::DO(dev) => dev.hw_cfg()
        }
    }

    fn hw_cfg_mut(&mut self) -> &mut HwCfg {
        match self {
            Self::AO(dev) => dev.hw_cfg_mut(),
            Self::DO(dev) => dev.hw_cfg_mut(),
        }
    }
}
// endregion