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
use std::fmt::Debug;
use std::sync::mpsc::{Sender, Receiver, SendError, RecvError};

use indexmap::IndexMap;
use itertools::Itertools;

use base_streamer::channel::BaseChan;
use base_streamer::device::BaseDev;
use base_streamer::streamer::TypedDev;

use crate::channel::{AOChan, DOChan, DOPortChan};
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

pub struct StreamBundle<T> {
    ni_task: NiTask,
    counter: StreamCounter,
    buf_write_timeout: Option<f64>,  // Some(finite_timeout_in_seconds) or None - wait infinitely
    samp_buf: Vec<T>,
    port_samp_buf: Option<Vec<u32>>,  // FixMe: this is a dirty hack:
    /*
        port_samp_buf is used by DODev for merging line samp_arr into port samp_arr.

        Since AO does not need it, it should not be contained in StreamBundle.
        The clean alternative - store it as a field in DODev. But a question there:
        - port_samp_buf Array1 will be stored in the main thread RAM while samp_arr - in worker thread RAM.
          Is accessing Array across threads slower?
    */
}

pub enum SampBufs {
    AO(Vec<f64>),
    DOLinesPorts((Vec<bool>, Vec<u32>)),
    DOPorts(Vec<u32>)
}

// pub struct SampBufs {
//
// }

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
    fn create_task_chans(&self, task: &NiTask) -> Result<(), DAQmxError>;
    fn alloc_samp_bufs(&self) -> SampBufs;
    fn calc_samps_(&self, samp_bufs: &mut SampBufs, start_pos: usize, end_pos: usize) -> Result<(), String>;
    fn write_buf(&self, stream_bundle: &mut StreamBundle<T>, samp_bufs: &SampBufs, samp_num: usize) -> Result<usize, DAQmxError>;
    //      to remove dependency on `trait BaseDev<T>` and remove type parameter `T`:
    // fn total_samps(self) -> usize;
    // self.compiled_chans().len()

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
        let mut stream_bundle = self.cfg_run_(bufsize_ms)?;
        report_sendr.send(())?;

        loop {
            match cmd_recvr.recv()? {
                WorkerCmd::Stream(calc_next) => {
                    self.stream_run_(&mut stream_bundle, &start_sync, calc_next)?;
                    report_sendr.send(())?;
                },
                WorkerCmd::Close => {
                    break
                }
            }
        };
        Ok(())
    }
    fn cfg_run_(&self, bufsize_ms: f64) -> Result<StreamBundle<T>, WorkerError> {
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
        let counter = StreamCounter::new(seq_len, buf_size);

        let samp_buf = vec![
            self.chans().values().next().unwrap().dflt_val();
            self.compiled_chans().len() * buf_size
        ];

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
            samp_buf,
            port_samp_buf: None,  // FixMe: this is a dirty hack.
            /* AO card will ignore `port_samp_buf`.
               DO card will set it to `Some(Array1<u32>)` the first time `self.write_buf()` is called
               and will re-use it for line-port merging.
            */
        };

        // Calc and write the initial sample chunk into the buffer
        let (start_pos, end_pos) = stream_bundle.counter.tick_next().unwrap();
        self.calc_samps_(&mut stream_bundle, start_pos, end_pos)?;
        // self.calc_samps(
        //     &mut stream_bundle,
        //     start_pos,
        //     end_pos
        // )?;
        self.write_buf(&mut stream_bundle, end_pos - start_pos)?;

        Ok(stream_bundle)
    }
    fn stream_run_(&self, stream_bundle: &mut StreamBundle<T>, start_sync: &StartSync, calc_next: bool) -> Result<(), WorkerError> {
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
            self.calc_samps(
                &mut stream_bundle.samp_buf[..],
                start_pos,
                end_pos
            )?;
            self.write_buf(stream_bundle, end_pos - start_pos)?;
        }

        // Now need to wait for the final sample chunk to be generated out by the card before stopping the task.
        // In the mean time, we can calculate the initial chunk for the next repetition in the case we are on repeat.
        if !calc_next {
            stream_bundle.ni_task.wait_until_done(stream_bundle.buf_write_timeout.clone())?;
            stream_bundle.ni_task.stop()?;
        } else {
            stream_bundle.counter.reset();
            let (start_pos, end_pos) = stream_bundle.counter.tick_next().unwrap();
            self.calc_samps(
                &mut stream_bundle.samp_buf[..],
                start_pos,
                end_pos
            )?;

            stream_bundle.ni_task.wait_until_done(stream_bundle.buf_write_timeout.clone())?;
            stream_bundle.ni_task.stop()?;

            self.write_buf(stream_bundle, end_pos - start_pos)?;
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

    fn write_buf(&self, stream_bundle: &mut StreamBundle<f64>, samp_num: usize) -> Result<usize, DAQmxError> {
        stream_bundle.ni_task.write_analog(
            &stream_bundle.samp_buf[..],
            samp_num,
            stream_bundle.buf_write_timeout.clone()
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
    port_chans: Option<IndexMap<String, DOPortChan>>
}

impl DODev {
    pub fn new(name: &str, samp_rate: f64) -> Self {
        Self {
            name: name.to_string(),
            samp_rate,
            chans: IndexMap::new(),
            hw_cfg: HwCfg::dflt(),
            const_fns_only: false,
            port_chans: None,
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

    pub fn compiled_port_nums(&self) -> Vec<usize> {
        self.compiled_chans()
            .iter()
            .map(|chan| chan.port())
            .unique()
            .sorted()
            .collect()
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

    fn compile(&mut self, stop_time: f64) -> Result<f64, String> {
        let total_run_time = BaseDev::compile_base(self, stop_time)?;

        // ToDo: line -> port merging

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
        for port_num in self.compiled_port_nums() {
            task.create_do_chan(&format!("/{}/port{}", self.name(), port_num))?;
        }
        Ok(())
    }

    fn write_buf(&self, bundle: &mut StreamBundle<bool>, samp_num: usize) -> Result<usize, DAQmxError> {
        // Sanity check - requested samp_num is not too large
        if bundle.samp_buf.len() < self.compiled_chans().len() * samp_num {
            return Err(DAQmxError::new(format!(
                "[write_buf()] BUG:\n\
                \tsamp_num * self.compiled_chans().len() = {} \n\
                is greater than \n\
                \tstream_bundle.samp_buf.len() = {}",
                samp_num * self.compiled_chans().len(),
                bundle.samp_buf.len()
            )))
        }

        // (1) Merge lines into ports
        match bundle.port_samp_buf.as_mut() {
            None => {
                // This is the first-time write_buf() call during cfg_run() - allocate the new Vec
                bundle.port_samp_buf = Some(vec![0; self.compiled_port_nums().len() * samp_num]);
            },
            Some(port_samp_buf) => {
                // This is a subsequent call of write_buf() after cfg_run() was completed.

                // Check that port_samp_buf is large enough (it should be unless there is some bug)
                let buf_size_needed = self.compiled_port_nums().len() * samp_num;
                if port_samp_buf.len() < buf_size_needed {
                    return Err(DAQmxError::new(format!(
                        "[write_buf()] BUG: port_samp_buf has insufficient size:\n\
                        \tport_samp_buf.len() = {}\n\
                        \tself.compiled_port_nums().len() * samp_num = {buf_size_needed}",
                        self.compiled_port_nums().len()
                    )))
                }

                // Clear previous values
                port_samp_buf.fill(0);
            }
        }
        let port_samp_buf = bundle.port_samp_buf.as_mut().unwrap();

        for (port_row_idx, &port_num) in self.compiled_port_nums().iter().enumerate() {
            let port_slice = &mut port_samp_buf[port_row_idx * samp_num .. (port_row_idx + 1) * samp_num];

            for (chan_row_idx, chan) in self.compiled_chans().iter().enumerate() {
                if chan.port() != port_num {
                    continue
                }
                let chan_slice = &bundle.samp_buf[chan_row_idx * samp_num .. (chan_row_idx + 1) * samp_num];
                let line_idx = chan.line();
                for (res, &samp) in port_slice.iter_mut().zip(chan_slice.iter()) {
                    *res |= (samp as u32) << line_idx;
                }
            }
        }

        // (2) Write to buffer
        let samps_written = bundle.ni_task.write_digital_port(
            &bundle.port_samp_buf.as_ref().unwrap()[..],
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