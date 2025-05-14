//! # NI Device Streaming and Control with the `experiment` Module
//!
//! This module is dedicated to providing a seamless interface for experiments that require direct
//! streaming to National Instruments (NI) devices. Building on the foundation of the
//! [`nicompiler_backend::Experiment`] struct, it introduces an extended `Experiment` struct which
//! offers enhanced functionalities tailored for NI device management.
//!
//! ## Key Features:
//!
//! - **NI Device Streaming:** With methods like [`Experiment::stream_exp`], you can start streaming processes
//!   for all compiled devices within an experiment concurrently. This ensures efficient use of resources and
//!   a smoother user experience.
//!
//! - **Device Reset Capabilities:** Provides methods such as [`Experiment::reset_device`] and
//!   [`Experiment::reset_devices`] to reset specific or all devices, ensuring they're brought back
//!   to a default or known state.
//!
//! - **Multi-threading Support:** The module employs multi-threading capabilities, specifically through the
//!   [`rayon`] crate, to handle concurrent device streaming.
//!
//! ## How to Use:
//!
//! 1. **Initialization:** Create an instance of the `Experiment` struct with the [`Experiment::new`] method.
//!    This gives you an experiment environment with no associated devices initially.
//!
//! 2. **Experiment design:** Design your interface using implemented methods in the [`nicompiler_backend::BaseExperiment`]
//! trait.
//!
//! 3. **Streaming and Control:** Use methods like [`Experiment::stream_exp`] to start the streaming process
//!    and [`Experiment::reset_device`] methods for device resets.
//!
//! ## Relationship with `nicompiler_backend`:
//!
//! This module's `Experiment` struct is an extended version of the [`nicompiler_backend::Experiment`].
//! While it offers NI-specific functionalities, for most general experiment behaviors, it leverages the
//! implementations in [`nicompiler_backend::BaseExperiment`]. Thus, users familiar with the compiler's
//! `Experiment` will find many commonalities here but with added advantages for NI device control.
//!
//! If your use-case doesn't involve direct interaction with NI devices, or you're looking for more general
//! experiment functionalities, please refer to the [`nicompiler_backend::Experiment`] for a broader scope.
//!
//! ## Further Reading:
//!
//! For more in-depth details and examples, refer to the individual struct and method documentations within this
//! module. Also, make sure to explore other related modules like [`device`], [`utils`] for comprehensive
//! device streaming behavior and NI-DAQmx specific operations, respectively.

use std::thread;
use std::thread::JoinHandle;
use std::sync::Arc;
use std::sync::mpsc::{Receiver, channel, RecvTimeoutError};
use std::time::Duration;
use std::collections::HashMap;

use parking_lot::Mutex;
use itertools::Itertools;
use indexmap::IndexMap;

use base_streamer::device::BaseDev;
use base_streamer::streamer::{TagBaseDev, BaseStreamer};

use crate::device::{AODev, DODev, NIDev, StartSync, WorkerError};
use crate::worker_cmd_chan::{CmdChan, WorkerCmd};
use crate::drop_alarm::new_drop_alarm;
use crate::nidaqmx;

#[derive(Debug)]
pub enum StreamStatus {
    Idle,
    Running,
}

pub struct StreamControls {
    worker_handles: IndexMap<String, JoinHandle<Result<(), WorkerError>>>,
    worker_cmd_chan: CmdChan,
    worker_report_recvrs: Vec<Receiver<()>>,
    stop_flag: Arc<Mutex<bool>>,
    reps_written_table: IndexMap<String, Arc<Mutex<usize>>>,
    // Internal variable to remember stream state:
    status: StreamStatus,
}

impl StreamControls {
    pub fn new() -> Self {
        Self {
            worker_handles: IndexMap::new(),
            worker_cmd_chan: CmdChan::new(),
            worker_report_recvrs: Vec::new(),
            stop_flag: Arc::new(Mutex::new(false)),
            reps_written_table: IndexMap::new(),
            status: StreamStatus::Idle,
        }
    }

    pub fn close_stream(mut self) -> Result<(), String> {
        // Command all workers to break out of in-stream loops and return.
        *self.stop_flag.lock() = true;
        self.worker_cmd_chan.send(WorkerCmd::Close);
        /*
            After that, any still-running worker will eventually return no matter what state it was in.
            Plus any failed ones have quit already by either returning an Err or panicking.
            So joining all handles should not lead to a deadlock.
        */

        // Join all worker threads. Collect all error messages, if any.
        let mut err_msgs = IndexMap::new();
        for (name, handle) in self.worker_handles.drain(..) {
            match handle.join() {
                Ok(res) => { match res {
                    Ok(()) => { /* Worker returned without errors */ }
                    Err(msg) => {
                        /* Worker gracefully returned an error */
                        err_msgs.insert(name, msg.to_string());
                    }
                }},
                Err(panic_obj) => {
                    /* Worker has panicked */
                    err_msgs.insert(name, format!("Panic message: {:?}", panic_obj));
                }
            }
        }
        // Return:
        if err_msgs.is_empty() {
            Ok(())
        } else {
            let mut joint_err_msg = String::new();
            for (name, msg) in err_msgs {
                joint_err_msg.push_str(&format!("[{name}] {msg}\n\n"))
            }
            Err(joint_err_msg)
        }
    }
}

/// An extended version of the [`nicompiler_backend::Experiment`] struct, tailored to provide direct
/// interfacing capabilities with NI devices.
///
/// This `Experiment` struct is designed to integrate closely with National Instruments (NI) devices,
/// providing methods to stream data to these devices and reset them as needed. It incorporates
/// multi-threading capabilities to handle concurrent device streaming and is equipped with helper
/// methods for device management.
///
/// The underlying `devices` IndexMap contains all the devices registered to this experiment, with
/// device names (strings) as keys mapping to their respective `Device` objects.
///
/// While this struct offers enhanced functionalities specific to NI device management,
/// for most general experiment behaviors, it relies on the implementation of [`nicompiler_backend::BaseExperiment`] in
/// [`nicompiler_backend::Experiment`]. Thus, users familiar with the compiler's `Experiment`
/// will find many similarities here, but with added methods to facilitate control over NI devices.
///
/// If you are not looking to interact directly with NI devices or if your use-case doesn't involve
/// NI-specific operations, refer to [`nicompiler_backend::Experiment`] for a more general-purpose
/// experimental control.
pub struct Streamer {
    devs: IndexMap<String, NIDev>,
    running_devs: IndexMap<String, Arc<Mutex<NIDev>>>,  // FixMe: this is a temporary dirty hack. Transfer device objects back to the main map (they were transferred out to be able to wrap them into Arc<Mutex<>> for multithreading)
    // Streamer-wide settings
    chunksize_ms: f64,
    ref_clk_provider: Option<(String, String)>,  // Some((dev_name, terminal_name)) or None
    starts_last: Option<String>,  // Some(dev_name) or None
    // Stream controls
    stream_controls: Option<StreamControls>,
}

impl BaseStreamer for Streamer {
    fn devs(&self) -> Vec<&dyn TagBaseDev> {
        self.devs
            .values()
            .map(|ni_dev| match ni_dev {
                NIDev::AO(dev) => dev as &dyn TagBaseDev,
                NIDev::DO(dev) => dev as &dyn TagBaseDev,
            })
            .collect()
    }
    fn devs_mut(&mut self) -> Vec<&mut dyn TagBaseDev> {
        self.devs
            .values_mut()
            .map(|ni_dev| match ni_dev {
                NIDev::AO(dev) => dev as &mut dyn TagBaseDev,
                NIDev::DO(dev) => dev as &mut dyn TagBaseDev,
            })
            .collect()
    }
}

impl Streamer {
    /// Constructor for the `Experiment` class.
    ///
    /// This constructor initializes an instance of the `Experiment` class with an empty collection of devices.
    /// The underlying representation of this collection is a IndexMap where device names (strings) map to their
    /// respective `Device` objects.
    ///
    /// # Returns
    /// - An `Experiment` instance with no associated devices.
    ///
    /// # Example (python)
    /// ```python
    /// from nicompiler_backend import Experiment
    ///
    /// exp = Experiment()
    /// assert len(exp.devices()) == 0
    /// ```
    pub fn new() -> Self {
        Self {
            devs: IndexMap::new(),
            running_devs: IndexMap::new(),  // FixMe: this is a temporary dirty hack. Transfer device objects back to the main map (they were transferred out to be able to wrap them into Arc<Mutex<>> for multithreading)
            // Streamer-wide settings
            chunksize_ms: 150.0,
            ref_clk_provider: None,  // Some((dev_name, terminal_name))
            starts_last: None,  // Some(dev_name)
            // Stream controls
            stream_controls: None,
        }
    }

    pub fn add_ao_dev(&mut self, dev: AODev) -> Result<(), String> {
        self.check_can_add_dev(dev.name())?;
        self.devs.insert(dev.name(), NIDev::AO(dev));
        Ok(())
    }

    pub fn add_do_dev(&mut self, dev: DODev) -> Result<(), String> {
        self.check_can_add_dev(dev.name())?;
        self.devs.insert(dev.name(), NIDev::DO(dev));
        Ok(())
    }

    pub fn dev_names(&self) -> Vec<String> {
        self.devs.keys().map(|name| name.clone()).collect()
    }

    pub fn borrow_dev(&self, name: String) -> Result<&NIDev, String> {
        if self.devs.keys().contains(&name) {
            Ok(self.devs.get(&name).unwrap())
        } else {
            Err(format!(
                "There is no device with name {name} registered. Registered devices are: {:?}",
                self.devs.keys()
            ))
        }
    }

    pub fn borrow_dev_mut(&mut self, name: String) -> Result<&mut NIDev, String> {
        if self.devs.keys().contains(&name) {
            Ok(self.devs.get_mut(&name).unwrap())
        } else {
            Err(format!(
                "There is no device with name {name} registered. Registered devices are: {:?}",
                self.devs.keys()
            ))
        }
    }

    pub fn get_chunksize_ms(&self) -> f64 {
        self.chunksize_ms.clone()
    }
    pub fn set_chunksize_ms(&mut self, val: f64) -> Result<(), String> {
        if val < 0.0 {
            return Err(format!("Negative chunksize_ms value provided: {val}"))
        }
        self.chunksize_ms = val;
        Ok(())
    }

    pub fn get_starts_last(&self) -> Option<String> {
        self.starts_last.clone()
    }
    pub fn set_starts_last(&mut self, name: Option<String>) {
        self.starts_last = name;
    }

    pub fn get_ref_clk_provider(&self) -> Option<(String, String)> {
        self.ref_clk_provider.clone()
    }
    pub fn set_ref_clk_provider(&mut self, provider: Option<(String, String)>) {
        self.ref_clk_provider = provider;
    }

    fn export_ref_clk_(&mut self) -> Result<(), String> {
        if let Some((dev_name, term_name)) = &self.ref_clk_provider {
            // ToDo: Maybe try tristating the terminal on all other cards in the streamer to ensure the line is not driven
            nidaqmx::connect_terms(
                &format!("/{dev_name}/10MHzRefClock"),
                &format!("/{dev_name}/{term_name}")
            ).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }
    pub fn undo_export_ref_clk_(&mut self) -> Result<(), String> {
        if let Some((dev_name, term_name)) = &self.ref_clk_provider {
            nidaqmx::disconnect_terms(
                &format!("/{dev_name}/10MHzRefClock"),
                &format!("/{dev_name}/{term_name}")
            ).map_err(|e| e.to_string())
        } else {
            Ok(())
        }
    }

    pub fn init_stream(&mut self) -> Result<(), String> {
        // (1) Sanity checks
        if self.stream_controls.is_some() {
            return Err("There is already a stream initialized".to_string())
        }

        if !self.got_instructions() {
            return Err("Streamer did not get any instructions".to_string())
        }
        self.validate_compile_cache()?;

        let active_dev_names = self.active_dev_names();
        if self.get_starts_last().is_some_and(|starts_last| !active_dev_names.contains(&starts_last)) {
            return Err(format!(
                "NIStreamer.starts_last is set to Some({}) but this name is not found in the active device list {active_dev_names:?}. \
                Either the name is invalid or this device didn't get any instructions and will not run at all",
                self.get_starts_last().unwrap()
            ))
        }

        // (2) Starting this point we begin making changes which will need to be reverted when closing the stream or if init fails

        // Export 10 MHz reference clock
        /*
            We are using static ref_clk export (as opposed to task-based export) to be able to always use
            the same card as the reference clock source even if this card does not run this time.
        */
        self.export_ref_clk_()?;

        // Transfer device objects to a separate IndexMap to wrap them into `Arc<Mutex<>>` for sharing with worker threads  // FixMe: this is a dirty hack.
        for name in active_dev_names {
            let dev = self.devs.shift_remove(&name).unwrap();
            self.running_devs.insert(name, Arc::new(Mutex::new(dev)));
        }
        let mut running_dev_clones = IndexMap::new();
        for (name, dev_container) in self.running_devs.iter() {
            running_dev_clones.insert(name.to_string(), dev_container.clone());
        };

        // Target duration of a single repetition - the longest compiled sequence duration among all running devs
        let target_rep_dur = self.running_devs.values()
            .map(|dev_mutex| dev_mutex.lock().compiled_stop_time())
            .reduce(|longest_so_far, this| f64::max(longest_so_far, this))
            .unwrap();
        let chunksize_ms = self.get_chunksize_ms();

        // New stream controls instance
        let mut stream_controls = StreamControls::new();

        // Drop alarm
        let mut alarm_handles = new_drop_alarm(self.running_devs.len());

        // Inter-worker start sync channels
        let mut start_sync = HashMap::new();
        if let Some(primary_dev_name) = &self.starts_last {
            // Create and pack sender-receiver pairs
            let mut recvr_vec = Vec::new();
            // - first create all the secondaries
            for dev_name in self.running_devs.keys().filter(|dev_name| dev_name.to_string() != primary_dev_name.clone()) {
                let (sender, recvr) = channel::<()>();
                recvr_vec.push(recvr);
                start_sync.insert(
                    dev_name.to_string(),
                    StartSync::Secondary(sender)
                );
            }
            // - now create the primary
            start_sync.insert(
                primary_dev_name.to_string(),
                StartSync::Primary(recvr_vec)
            );
        } else {
            for dev_name in self.running_devs.keys() {
                start_sync.insert(
                    dev_name.to_string(),
                    StartSync::None,
                );
            }
        }

        // For each worker, prepare a few more individual controls and launch the thread
        let mut spawn_result: Result<(), String> = Ok(());
        for (dev_name, dev_container) in running_dev_clones.drain(..) {
            // - command receiver for this worker
            let cmd_recvr = stream_controls.worker_cmd_chan.new_recvr();

            // - report channel for this worker
            let (report_sender, report_recvr) = channel::<()>();
            stream_controls.worker_report_recvrs.push(report_recvr);

            // - retrieve start_sync
            let worker_start_sync = start_sync.remove(&dev_name).unwrap();

            let stop_flag = stream_controls.stop_flag.clone();
            let alarm_handle = alarm_handles.pop().unwrap();

            // - reps written counter
            let reps_written = Arc::new(Mutex::new(0));
            stream_controls.reps_written_table.insert(dev_name.clone(), reps_written.clone());

            // Launch worker thread
            let res = thread::Builder::new()
                .name(dev_name.clone())
                .spawn(move || {
                    dev_container
                        .lock()
                        .worker_loop(chunksize_ms, cmd_recvr, report_sender, worker_start_sync, stop_flag, alarm_handle, reps_written, target_rep_dur)
                });
            match res {
                Ok(join_handle) => { stream_controls.worker_handles.insert(dev_name, join_handle); },
                Err(io_err) => {
                    spawn_result = Err(io_err.to_string());
                    break
                },
            }
        }
        if let Err(spawn_msg) = spawn_result {
            let worker_report = stream_controls.close_stream();
            self.undo_init_changes();
            return Err(format!("Thread spawn failed: {spawn_msg}. In addition, joint report from all spawned workers (all were shut down): {worker_report:?}"))
        }

        // (3) Wait for all workers to report init completion and then return.
        // If anyone fails (error/panic), it will be visible as `Err` on trying to receive the report.
        let all_completed = stream_controls.worker_report_recvrs
            .iter()
            .all(|recvr| recvr.recv().is_ok());

        if all_completed {
            self.stream_controls = Some(stream_controls);
            Ok(())
        } else {
            // Shut down the stream
            let worker_report = stream_controls.close_stream();
            self.undo_init_changes();
            // Return an error
            if let Err(msg) = worker_report {
                Err(msg)
            } else {
                Err("[BUG] Some workers have quit unexpectedly at init, yet no worker reported an error".to_string())
            }
        }
    }

    pub fn launch(&mut self, nreps: usize) -> Result<(), String> {
        // (1) Status checks
        if self.stream_controls.is_none() {
            return Err("Stream is not initialized".to_string())
        }
        let controls = self.stream_controls.as_mut().unwrap();
        match controls.status {
            StreamStatus::Idle => { /* good to go */ },
            StreamStatus::Running => return Err("Stream is already running".to_string()),
        }
        // (2) Command all workers to launch
        *controls.stop_flag.lock() = false;
        controls.reps_written_table.values().for_each(|counter| *counter.lock() = 0);
        controls.worker_cmd_chan.send(WorkerCmd::Run(nreps));
        controls.status = StreamStatus::Running;

        Ok(())
    }

    pub fn request_stop(&self) -> Result<(), String> {
        if let Some(controls) = &self.stream_controls {
            *controls.stop_flag.lock() = true;
            Ok(())
        } else {
            Err("Stream is not initialized".to_string())
        }
    }

    pub fn wait_until_finished(&mut self, timeout: Duration) -> Result<bool, String> {
        if self.stream_controls.is_none() {
            return Err("Stream is not initialized".to_string())
        };
        let controls = self.stream_controls.as_mut().unwrap();
        match controls.status {
            StreamStatus::Running => { /* Should wait. Moving further in logic */ },
            StreamStatus::Idle => return Ok(true),  // Stream is idle, return immediately.
        }

        // To begin, try to receive the report from the first worker using `recv_timeout()`.
        // Act depending on this first return:
        // * Timeout - the first worker is still busy and no peer drop was detected so far - return timeout;
        // * Disconnected - proceed to closing stream;
        // * Reported completion - other workers should be finishing soon as well.
        //                         Wait for them to report without timeout limit.
        let mut failed = false;
        match controls.worker_report_recvrs.first().unwrap().recv_timeout(timeout) {
            Err(RecvTimeoutError::Timeout) => return Ok(false),
            Err(RecvTimeoutError::Disconnected) => { failed = true },
            Ok(()) => {
                failed = controls.worker_report_recvrs[1..]
                    .iter()
                    .any(|recvr| recvr.recv().is_err());
            },
        };

        if !failed {
            controls.status = StreamStatus::Idle;
            Ok(true)
        } else {
            // Shut down the stream
            let controls = self.stream_controls.take().unwrap();  // Also sets `self.stream_controls` to `None`
            let worker_report = controls.close_stream();
            self.undo_init_changes();
            // Return an error
            if let Err(msg) = worker_report {
                Err(msg)
            } else {
                Err("[BUG] Some workers have quit unexpectedly, yet no worker reported an error".to_string())
            }
        }
    }

    /// Returns the number of waveform repetitions (per device) that all devices have fully sampled
    /// and written into the onboard buffer already (some devices may have written more than that).
    ///
    /// **WARNING:** this number does not precisely reflect how many repetitions have been **generated out** already.
    /// Samples are calculated and written to devices some time ahead of when they will actually be output
    /// (this is necessary for stream stability). This function tracks how many has been _calculated and written_
    /// rather than how many has been _actually output_.
    ///
    /// So this value should only be used as a coarse progress indicator for monitoring purposes only,
    /// it is not suitable as a sync mechanism.
    pub fn reps_written_count(&self) -> Result<usize, String> {
        if let Some(controls) = &self.stream_controls {
            let lowest_count = controls.reps_written_table.values()
                .map(|counter| counter.lock().clone())
                .reduce(|lowest_so_far, this| usize::min(lowest_so_far, this))
                .unwrap();
            Ok(lowest_count)
        } else {
            Err("Stream is not initialized".to_string())
        }
    }

    pub fn close_stream(&mut self) -> Result<(), String> {
        if self.stream_controls.is_none() {
            Ok(())
        } else {
            let controls = self.stream_controls.take().unwrap();  // Also sets `self.stream_controls` to `None`
            let worker_report = controls.close_stream();
            self.undo_init_changes();
            worker_report
        }
    }

    fn undo_init_changes(&mut self) {
        // Undo 10 MHz reference export
        /* This method is meant to be called after `Self::export_ref_clk_()` was called and succeeded.
           If exporting clock had no errors, `dev_name` and `term_name` should be correct
           and we do not expect this call to fail. */
        self.undo_export_ref_clk_().unwrap();

        // Return all active device objects back to the main IndexMap
        /* This method is meant to be called after `manager` thread was joined so all workers
           must have returned and dropped their clones of `Arc<Mutex<dev>>` earlier. */
        for (dev_name, dev_box) in self.running_devs.drain(..) {
            let dev = Arc::into_inner(dev_box).unwrap().into_inner();
            self.devs.insert(dev_name, dev);
        };
    }

    pub fn reset_all(&self) -> Result<(), String> {
        for dev_name in self.devs.keys() {
            nidaqmx::reset_device(dev_name).map_err(|e| e.to_string())?
        };
        Ok(())
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        let res = self.close_stream();
        if let Err(msg) = res {
            println!("Error when dropping NIStreamer: {msg}")
        };
    }
}
