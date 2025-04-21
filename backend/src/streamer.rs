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

use std::collections::HashMap;
use std::thread;
use std::thread::JoinHandle;
use std::sync::Arc;
use std::sync::mpsc::{Sender, Receiver, channel, RecvTimeoutError};
use std::time::Duration;

use parking_lot::Mutex;
use itertools::Itertools;
use indexmap::IndexMap;

use base_streamer::device::BaseDev;
use base_streamer::streamer::{TagBaseDev, BaseStreamer};

use crate::device::{AODev, DODev, NIDev, RunControl, StartSync, WorkerError};
use crate::manager_thread::{manager_loop, ManagerCmd, ManagerErr};
use crate::nidaqmx;
use crate::nidaqmx::DAQmxError;
use crate::worker_cmd_chan::{CmdChan, CmdRecvr, WorkerCmd};

#[derive(Debug)]
pub enum StreamStatus {
    Ready,
    Running(Arc<Mutex<usize>>),  // "reps written" counter
}

pub struct StreamControls {
    manager_handle: JoinHandle<Result<(), String>>,
    manager_cmd_sender: Sender<ManagerCmd>,
    manager_report_recvr: Receiver<()>,
    stop_requested: Arc<Mutex<bool>>,
    // Internal variable to remember stream state:
    status: StreamStatus,
}

pub enum WaitUntilFinishedErr {
    Timeout,
    Failed(String),
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
    // Worker thread communication objects
    worker_cmd_chan: CmdChan,
    worker_report_recvrs: IndexMap<String, Receiver<()>>,
    worker_handles: IndexMap<String, JoinHandle<Result<(), WorkerError>>>,  // FixMe [after Device move to streamer crate]: Maybe store individual device thread handle and report receiver in the Device struct
    // Stream manager controls
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
            // Worker thread communication objects
            worker_cmd_chan: CmdChan::new(),
            worker_report_recvrs: IndexMap::new(),
            worker_handles: IndexMap::new(),  // FixMe [after Device move to streamer crate]: Maybe store individual device thread handle and report receiver in the Device struct
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

    fn export_ref_clk_(&mut self) -> Result<(), DAQmxError> {
        if let Some((dev_name, term_name)) = &self.ref_clk_provider {
            // ToDo: Try tristating the terminal on all other cards in the streamer to ensure the line is not driven
            nidaqmx::connect_terms(
                &format!("/{dev_name}/10MHzRefClock"),
                &format!("/{dev_name}/{term_name}")
            )?;
        };
        Ok(())
    }
    pub fn undo_export_ref_clk_(&mut self) -> Result<(), DAQmxError> {
        if let Some((dev_name, term_name)) = &self.ref_clk_provider {
            nidaqmx::disconnect_terms(
                &format!("/{dev_name}/10MHzRefClock"),
                &format!("/{dev_name}/{term_name}")
            )?;
        };
        Ok(())
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

        // (2) Prepare to launch manager thread

        // Export 10 MHz reference clock
        /* We are using static ref_clk export (as opposed to task-based export) to be able to always use
        the same card as the reference clock source even if this card does not run this time. */
        self.export_ref_clk_().map_err(|daqmx_err| daqmx_err.to_string())?;

        // Transfer device objects to a separate IndexMap to wrap them into `Arc<Mutex<>>` for sharing with worker threads  // FixMe: this is a dirty hack.
        for dev_name in active_dev_names {
            let dev = self.devs.shift_remove(&dev_name).unwrap();
            self.running_devs.insert(dev_name, Arc::new(Mutex::new(dev)));
        }
        // Make a collection of `Arc` clones to be eventually passed into worker threads
        let running_devs_clone: IndexMap<String, Arc<Mutex<NIDev>>> = self.running_devs
            .iter()
            .map(|(name, arc_mutex_dev)| (name.clone(), arc_mutex_dev.clone()))
            .collect();
        let starts_last_clone = self.get_starts_last();

        // Channels for communication between `main` and `manager` threads
        let (cmd_sender, cmd_recvr) = channel::<ManagerCmd>();
        let (report_sender, report_recvr) = channel::<()>();
        let stop_requested = Arc::new(Mutex::new(false));
        let stop_requested_clone = stop_requested.clone();

        // (3) Launch manager thread
        let chunksize_ms = self.chunksize_ms.clone();
        let spawn_result = thread::Builder::new()
            .name("Manager".to_string())
            .spawn(move || {manager_loop(
                running_devs_clone,
                cmd_recvr,
                report_sender,
                stop_requested_clone,
                starts_last_clone,
                chunksize_ms,
            )});
        if spawn_result.is_err() {
            // Most likely OS failed to spawn the thread
            self.undo_init_changes();
            return Err(spawn_result.unwrap_err().to_string())
        }
        let manager_handle = spawn_result.unwrap();

        // (4) Save all stream controls
        self.stream_controls = Some(StreamControls {
            manager_handle,
            manager_cmd_sender: cmd_sender,
            manager_report_recvr: report_recvr,
            stop_requested,
            status: StreamStatus::Ready,
        });

        // (5) Now wait for the manager to launch all the worker threads and all workers to init hardware.
        /*
            Manager will collect reports from all workers.
            - If all succeed, `manager` reports success back to `main` and starts waiting for commands.
              Success is reported by sending `()` back to `main`.

            - If any worker fails, the `manager` will automatically clean up all remaining workers,
              and will finally return leaving the error info in the returned message.
              The `main` thread will know about the failure by getting an error on trying to receive the report
              because manager drops its' side of the channel when quitting.
              Then `main` only needs to join the already-returned `manager` handle to collect the error message.
        */
        let recv_res = self.stream_controls.as_ref().unwrap().manager_report_recvr.recv();
        if recv_res.is_ok() {
            /* Manager reported init completion. All workers succeeded, stream is ready! */
            Ok(())
        } else {
            /* Manager has quit because some errors occurred.
               Manager was responsible for shutting down all the workers and cleaning up before returning.
               So just join manager thread to collect the returned error message and undo init changes. */
            let err_msg = self.collect_err_info_undo_init_changes();
            Err(err_msg)
        }
    }

    pub fn launch_run(&mut self, nreps: usize) -> Result<(), String> {
        // (1) Status checks
        if self.stream_controls.is_none() {
            return Err("Stream is not initialized".to_string())
        }
        let controls = self.stream_controls.as_mut().unwrap();
        match controls.status {
            StreamStatus::Ready => { /* good to go */ },
            StreamStatus::Running(_) => return Err("Stream is already running".to_string()),
        }

        // (2) Command manager to launch the run
        *controls.stop_requested.lock() = false;
        let reps_written_count = Arc::new(Mutex::new(0));
        let send_res = controls.manager_cmd_sender.send(
            ManagerCmd::Launch(nreps, reps_written_count.clone())
        );

        // (3) Handle the case the manager has quit already and return
        if send_res.is_ok() {
            controls.status = StreamStatus::Running(reps_written_count);
            Ok(())
        } else {
            // Manager thread dropped its' side of the command channel - it must have quit due to an error.
            let err_msg = self.collect_err_info_undo_init_changes();
            Err(err_msg)
        }
    }

    pub fn request_stop(&self) -> Result<(), String> {
        if self.stream_controls.is_none() {
            return Err("Stream is not initialized".to_string())
        }
        let controls = self.stream_controls.as_ref().unwrap();
        match controls.status {
            StreamStatus::Running(_) => {
                *controls.stop_requested.lock() = true;
                Ok(())
            },
            StreamStatus::Ready => Ok(()),
        }
    }

    pub fn wait_until_finished(&mut self, timeout: Duration) -> Result<(), WaitUntilFinishedErr> {
        if self.stream_controls.is_none() {
            return Err(WaitUntilFinishedErr::Failed("Stream is not initialized".to_string()))
        };
        let controls = self.stream_controls.as_mut().unwrap();
        match controls.status {
            StreamStatus::Running(_) => { /* Should wait. Moving further in logic */ },
            StreamStatus::Ready => return Ok(()),  // Stream is idle, return immediately.
        }

        match controls.manager_report_recvr.recv_timeout(timeout) {
            Ok(()) => {
                controls.status = StreamStatus::Ready;
                Ok(())
            },
            Err(recv_err) => match recv_err {
                RecvTimeoutError::Timeout => Err(WaitUntilFinishedErr::Timeout),
                RecvTimeoutError::Disconnected => {
                    // Manager thread dropped its' side of the command channel - it must have quit due to an error.
                    // Error info should be contained in the returned value - use join handle to retrieve it.
                    let err_msg = self.collect_err_info_undo_init_changes();
                    Err(WaitUntilFinishedErr::Failed(err_msg))
                }
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
        if self.stream_controls.is_none() {
            return Err("Stream is not initialized".to_string())
        }
        let controls = self.stream_controls.as_ref().unwrap();
        match &controls.status {
            StreamStatus::Running(reps_written_count) => Ok(reps_written_count.lock().clone()),
            StreamStatus::Ready => Err("Stream is not currently running".to_string())
        }
    }

    pub fn close_stream(&mut self) -> Result<(), String> {
        if self.stream_controls.is_none() {
            Ok(())
        } else {
            let controls = self.stream_controls.take().unwrap();  // also sets `self.stream_controls` to `None`
            // Command `manager` to stop any run loop and return no matter what state it was in originally
            *controls.stop_requested.lock() = true;
            let _ = controls.manager_cmd_sender.send(ManagerCmd::Close);
            // After these commands `manager` thread should eventually join
            let collected_res = match controls.manager_handle.join() {
                Ok(manager_res) => manager_res,
                Err(panic_info) => Err(format!("Manager has panicked: {panic_info:?}")),
            };
            self.undo_init_changes();
            collected_res
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

    /// This method should be used when stream manager has quit due to some errors occurring.
    ///
    /// Manager was responsible for shutting down all the workers and cleaning up before returning.
    /// So just undo init changes and join manager thread to collect the returned error message.
    ///
    /// **Warning**: this method will block until manager thread joins.
    /// Only call it if you are sure the manager has stopped already or will stop later.
    fn collect_err_info_undo_init_changes(&mut self) -> String {
        let controls = self.stream_controls.take().unwrap();  // also sets `self.stream_controls` to `None`
        let err_msg = match controls.manager_handle.join() {
            Ok(manager_res) => match manager_res {
                Err(err_info) => err_info,
                Ok(()) => "[BUG] Manager has quit unexpectedly, yet returned Ok".to_string(),
            },
            Err(panic_info) => format!("Manager has panicked: {panic_info:?}")
        };
        self.undo_init_changes();
        err_msg
    }

    fn collect_worker_reports(&mut self) -> Result<(), String> {
        // Wait for each worker thread to report completion or stop working (by returning or panicking)
        let mut failed_worker_names = Vec::new();
        for (dev_name, recvr) in self.worker_report_recvrs.iter() {
            match recvr.recv() {
                Ok(()) => {},
                Err(_err) => { failed_worker_names.push(dev_name.to_string())},
            };
        };
        if failed_worker_names.is_empty() {
            return Ok(())
        }

        // If any of the workers did not report Ok, they must have stopped
        // either by gracefully returning a WorkerError or by panicking.
        // Collect info from all failed workers and return the full error message.

        // For each failed worker:
        // * dispose of worker_report_receiver
        // * join the thread to collect error info [join() will automatically consume and dispose of the thread handle]
        // * transfer Device object from `self.running_devs` back to the main `self.devs`  // FixMe: this is a temporary dirty hack. Transfer device objects back to the main map (they were transferred out to be able to wrap them into Arc<Mutex<>> for multithreading)
        let mut err_msg_map = IndexMap::new();
        for dev_name in failed_worker_names.iter() {
            self.worker_report_recvrs.shift_remove(dev_name).unwrap();

            let join_handle = self.worker_handles.shift_remove(dev_name).unwrap();
            match join_handle.join() {
                Ok(worker_result) => {
                    // The worker had an error but returned gracefully. The WorkerError should be contained in the return result
                    match worker_result {
                        Err(worker_error) => err_msg_map.insert(dev_name.to_string(), worker_error.to_string()),
                        Ok(()) => err_msg_map.insert(dev_name.to_string(), format!("Unexpected scenario - worker has dropped its report_sender yet returned Ok")),
                    }
                },
                Err(_panic_info) => {
                    // The worker has panicked. Panic info should be contained in the returned object
                    err_msg_map.insert(dev_name.to_string(), format!("Worker has panicked"))
                },
            };

            let dev_container = self.running_devs.shift_remove(dev_name).unwrap();
            let dev = Arc::into_inner(dev_container).unwrap().into_inner();  // this line extracts Dev instance from Arc<Mutex<>> container
            self.devs.insert(dev_name.to_string(), dev);
        }

        // Assemble and return the full error message string
        let mut full_err_msg = String::new();
        for (dev_name, err_msg) in err_msg_map {
            full_err_msg.push_str(&format!(
                "[{dev_name}] {err_msg}\n"
            ))
        }
        // println!("[collect_thread_reports()] list of failed threads: {:?}", failed_worker_names);
        // println!("[collect_thread_reports()] full error message:\n{}", full_err_msg);
        Err(full_err_msg)
    }

    pub fn cfg_run_(&mut self, bufsize_ms: f64) -> Result<(), String> {
        if !self.got_instructions() {
            return Err("Streamer did not get any instructions".to_string())
        }
        self.validate_compile_cache()?;

        let active_dev_names: Vec<String> = self.devs
            .iter()
            .filter(|(_name, typed_dev)| match typed_dev {
                NIDev::AO(dev) => dev.got_instructions(),
                NIDev::DO(dev) => dev.got_instructions(),
            })
            .map(|(name, _typed_dev)| name.to_string())
            .collect();
        /* ToDo: Maybe add a consistency check here: no clash between any exports (ref clk, start_trig, samp_clk) */

        // Prepare thread sync mechanisms

        // - command broadcasting channel
        self.worker_cmd_chan = CmdChan::new();  // the old instance can be reused, but refreshing here to zero `msg_num` for simplicity

        // - inter-worker start sync channels
        let mut start_sync = HashMap::new();
        if let Some(primary_dev_name) = self.get_starts_last() {
            // Sanity check
            if !active_dev_names.contains(&primary_dev_name) {
                return Err(format!(
                    "NIStreamer.starts_last is set to Some({primary_dev_name}) but this name is not found in the running device list: \n\
                    {:?}\n\
                    Either name {primary_dev_name} is invalid or this device didn't get any instructions and will not run at all",
                    active_dev_names
                ))
            };

            // Create and pack sender-receiver pairs
            let mut recvr_vec = Vec::new();
            // - first create all the secondaries
            for dev_name in active_dev_names.iter().filter(|dev_name| dev_name.to_string() != primary_dev_name.clone()) {
                let (sender, recvr) = channel();
                recvr_vec.push(recvr);
                start_sync.insert(
                    dev_name.to_string(),
                    StartSync::Secondary(sender)
                );
            }
            // - now create the primary
            start_sync.insert(
                primary_dev_name,
                StartSync::Primary(recvr_vec)
            );
        } else {
            for dev_name in active_dev_names.iter() {
                start_sync.insert(
                    dev_name.to_string(),
                    StartSync::None,
                );
            }
        }

        // Get the target duration of a single repetition - the longest compiled stop time among all active devs
        // (get this value before moving the devices to the `running_devs` IndexMap)
        let target_rep_dur = self.longest_dev_run_time();

        // FixMe: this is a temporary dirty hack.
        //  Transfer device objects to a separate IndexMap to be able to wrap them into Arc<Mutex<>> for multithreading
        for dev_name in active_dev_names {
            let dev = self.devs.shift_remove(&dev_name).unwrap();
            self.running_devs.insert(dev_name, Arc::new(Mutex::new(dev)));
        }

        // Do static ref clk export
        /* We are using static ref_clk export (as opposed to task-based export) to be able to always use
        the same card as the clock reference source even if this card does not run this time. */
        if let Err(daqmx_err) = self.export_ref_clk_() {
            return Err(daqmx_err.to_string())
        };

        // Prepare a few more inter-thread sync objects and launch worker threads
        for (dev_name, dev_container) in self.running_devs.iter() {
            // - worker command receiver
            let cmd_recvr = self.worker_cmd_chan.new_recvr();

            // - worker report channel
            let (report_sendr, report_recvr) = channel();
            self.worker_report_recvrs.insert(dev_name.to_string(), report_recvr);

            // Launch worker thread
            let handle = Streamer::launch_worker_thread(
                dev_name.to_string(),
                dev_container.clone(),
                bufsize_ms,
                cmd_recvr,
                report_sendr,
                start_sync.remove(dev_name).unwrap(),
                target_rep_dur,
            )?;
            self.worker_handles.insert(dev_name.to_string(), handle);
        }
        // Wait for all workers to report config completion (handle error collection if necessary)
        self.collect_worker_reports()
    }
    fn launch_worker_thread(
        dev_name: String,
        dev_mutex: Arc<Mutex<NIDev>>,
        bufsize_ms: f64,
        cmd_recvr: CmdRecvr,
        report_sendr: Sender<()>,
        start_sync: StartSync,
        target_rep_dur: f64,
    ) -> Result<JoinHandle<Result<(), WorkerError>>, String> {
        let spawn_result = thread::Builder::new()
            .name(dev_name)
            .spawn(move || {
                let mut typed_dev = dev_mutex.lock();
                // match &mut *typed_dev {
                //     NIDev::AO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sendr, start_sync, target_rep_dur),
                //     NIDev::DO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sendr, start_sync, target_rep_dur),
                // }
                Ok(())
            });
        match spawn_result {
            Ok(handle) => Ok(handle),
            Err(err) => Err(err.to_string())
        }
    }
    pub fn stream_run_(&mut self, calc_next: bool, nreps: usize) -> Result<(), String> {
        // self.worker_cmd_chan.send(WorkerCmd::Run(calc_next, nreps));
        self.collect_worker_reports()
    }
    pub fn close_run_(&mut self) -> Result<(), String> {
        // Command all workers to break out of the event loop and return
        self.worker_cmd_chan.send(WorkerCmd::Close);

        // Join all worker threads
        //  At this point it is expected that all workers should just join cleanly and each return Ok(()):
        //      if there were any errors during `cfg_run_()` or `stream_run_()` calls,
        //      those threads should have been handled and removed from `self.worker_handles` during `collect_worker_reports()` calls
        //
        //  So if now any of the remaining workers doesn't join or joins but returns a WorkerError
        //  - this is something very unexpected. Try to join all other threads first and launch a panic at the end.
        let mut worker_join_err_msgs = IndexMap::new();
        for (dev_name, handle) in self.worker_handles.drain(..) {
            match handle.join() {
                Ok(worker_result) => {
                    match worker_result {
                        Ok(()) => {/* this is the only option that we expect */},
                        Err(worker_error) => {
                            // The worker has returned gracefully but the return is a WorkerError
                            worker_join_err_msgs.insert(dev_name, worker_error.to_string());
                        },
                    }
                },
                Err(_panic_info) => {
                    // The worker has panicked
                    worker_join_err_msgs.insert(dev_name, format!("The worker appears to have panicked"));
                },
            };
        }

        // Dispose of all worker report receivers
        self.worker_report_recvrs.clear();

        // Undo static reference clock export
        let ref_clk_exp_undo_result = self.undo_export_ref_clk_();

        // FixMe: this is a dirty hack.
        //  Transfer device objects to a separate IndexMap to be able to wrap them into Arc<Mutex<>> for multithreading
        // Return all used device objects back to the main IndexMap
        for (dev_name, dev_box) in self.running_devs.drain(..) {
            let dev = Arc::into_inner(dev_box).unwrap().into_inner();
            self.devs.insert(dev_name, dev);
        }

        // Finally, return
        if worker_join_err_msgs.is_empty() && ref_clk_exp_undo_result.is_ok() {
            // println!("[clear_run()] joined all threads. Completed clearing run. Returning");
            return Ok(());
        }
        //  If any unexpected error has occurred:
        //  * some workers unexpectedly failed
        //  * static ref_clk export undoing has failed,
        //  assemble and return the full error message string
        let mut full_err_msg = String::new();
        full_err_msg.push_str("Error during closing run:\n");
        if let Err(daqmx_err) = ref_clk_exp_undo_result {
            full_err_msg.push_str(&format!("Failed to undo static reference clock export: {}\n", daqmx_err.to_string()));
        }
        for (dev_name, err_msg) in worker_join_err_msgs {
            full_err_msg.push_str(&format!(
                "[{dev_name}] {err_msg}\n"
            ))
        }
        Err(full_err_msg)
    }

    pub fn run(&mut self, nreps: usize, bufsize_ms: f64) -> Result<(), String> {
        // Group `cfg_run_()` and `stream_run_()` into one closure for convenient interruption in an error case
        let mut run_ = || -> Result<(), String> {
            self.cfg_run_(bufsize_ms)?;
            self.stream_run_(true, nreps)?;
            Ok(())
        };
        // The actual run:
        let run_result = run_();
        let close_result = self.close_run_();

        // Return result
        if run_result.is_ok() && close_result.is_ok() {
            Ok(())
        } else {
            let mut full_err_msg = String::new();
            if let Err(run_err_msg) = run_result {
                full_err_msg.push_str(&run_err_msg);
                full_err_msg.push_str("\n");
            };
            if let Err(close_err_msg) = close_result {
                full_err_msg.push_str(&close_err_msg);
                full_err_msg.push_str("\n");
            }
            Err(full_err_msg)
        }
    }

    pub fn reset_all(&self) -> Result<(), String> {
        // Pack the loop into a closure for convenience:
        //   to catch DAQmxError at any iteration and later convert it to String before returning
        let closure = || -> Result<(), DAQmxError> {
            for dev_name in self.devs.keys() {
                nidaqmx::reset_device(dev_name)?
            };
            Ok(())
        };
        match closure() {
            Ok(()) => Ok(()),
            Err(daqmx_err) => Err(daqmx_err.to_string())
        }
    }
}

impl Drop for Streamer {
    fn drop(&mut self) {
        let res = self.close_run_();
        if let Err(msg) = res {
            println!("Error when dropping NIStreamer: {msg}")
        };
    }
}
