use std::collections::HashMap;
use std::thread;
use std::thread::JoinHandle;
use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver, SendError, RecvError};
use parking_lot::Mutex;
use indexmap::IndexMap;

use crate::device::{NIDev, StartSync, WorkerReport, WorkerError};
use crate::worker_cmd_chan::{CmdChan, WorkerCmd};

pub enum ManagerCmd {
    Launch(usize),  // `usize` - nreps
    Close,
}

pub struct ManagerErr {
    msg: String
}
impl ManagerErr {
    pub fn new(msg: String) -> Self {
        Self { msg }
    }
}
impl From<SendError<()>> for ManagerErr {
    fn from(value: SendError<()>) -> Self {
        Self::new(format!("Manager thread encountered SendError: {}", value.to_string()))
    }
}
impl From<RecvError> for ManagerErr {
    fn from(value: RecvError) -> Self {
        Self::new(format!("Manager thread encountered RecvError: {}", value.to_string()))
    }
}
impl From<std::io::Error> for ManagerErr {
    fn from(value: std::io::Error) -> Self {
        Self::new(value.to_string())
    }
}
impl From<String> for ManagerErr {
    fn from(value: String) -> Self {
        Self::new(value)
    }
}
impl ToString for ManagerErr {
    fn to_string(&self) -> String {
        self.msg.clone()
    }
}

pub fn manager_loop(
    devs: IndexMap<String, Arc<Mutex<NIDev>>>,
    cmd_recvr: Receiver<ManagerCmd>,
    report_sender: Sender<()>,
    main_requested_stop: Arc<Mutex<bool>>,
    reps_written_count: Arc<Mutex<usize>>,
    starts_last: Option<String>,
    chunksize_ms: f64,
) -> Result<(), String> {
    // Init
    let mut manager = StreamManager::new_uninit();

    // Convenience closure to simplify error handling
    // - using `?` operator will return immediately if an error occurs at any step.
    let catch_closure = || -> Result<(), ManagerErr> {
        manager.try_init(devs, starts_last, chunksize_ms)?;
        report_sender.send(())?;

        loop {
            match cmd_recvr.recv()? {
                ManagerCmd::Launch(nreps) => {
                    manager.run(nreps, &main_requested_stop, &reps_written_count)?;
                    report_sender.send(())?;
                },
                ManagerCmd::Close => {
                    break
                }
            }
        }
        Ok(())
    };

    // Run the actual manager loop logic:
    let closure_result = catch_closure();

    // Shut down all workers (no matter what main logic result was)
    let worker_results = manager.shut_down_all_workers();

    // Return
    if closure_result.is_ok() && worker_results.is_ok() {
        Ok(())
    } else {
        let mut joint_err_msg = String::new();
        if let Err(msg) = worker_results {
            joint_err_msg.push_str(&format!("\nWorker error reports: \n\n{msg}"))
        }
        if let Err(err) = closure_result {
            joint_err_msg.push_str(&format!("\nManager error report was printed to console.\n"));
            println!("Manager error report: {}", err.to_string())
        }
        Err(joint_err_msg)
    }
}

pub struct StreamManager {
    worker_handles: IndexMap<String, JoinHandle<Result<(), WorkerError>>>,
    worker_cmd_chan: CmdChan,  // command broadcasting channel
    worker_report_recvrs: IndexMap<String, Receiver<WorkerReport>>,
    stop_flag: Arc<Mutex<bool>>,
}

impl StreamManager {
    pub fn new_uninit() -> Self {
        Self {
            worker_handles: IndexMap::new(),
            worker_cmd_chan: CmdChan::new(),
            worker_report_recvrs: IndexMap::new(),
            stop_flag: Arc::new(Mutex::new(false)),
        }
    }

    pub fn try_init(
        &mut self,
        mut devs: IndexMap<String, Arc<Mutex<NIDev>>>,
        starts_last: Option<String>,
        chunksize_ms: f64,
    ) -> Result<(), ManagerErr> {
        // Inter-worker start sync channels
        let mut start_sync = HashMap::new();
        if let Some(primary_dev_name) = starts_last {
            // Create and pack sender-receiver pairs
            let mut recvr_vec = Vec::new();
            // - first create all the secondaries
            for dev_name in devs.keys().filter(|dev_name| dev_name.to_string() != primary_dev_name.clone()) {
                let (sender, recvr) = channel::<()>();
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
            for dev_name in devs.keys() {
                start_sync.insert(
                    dev_name.to_string(),
                    StartSync::None,
                );
            }
        }

        // Target duration of a single repetition - the longest compiled sequence duration among all running devs
        let target_rep_dur = devs.values()
            .map(|dev_mutex| dev_mutex.lock().compiled_stop_time())
            .reduce(|longest_so_far, this| f64::max(longest_so_far, this))
            .unwrap();

        // Prepare a few more inter-thread sync objects and launch worker threads
        for (dev_name, dev_container) in devs.drain(..) {
            // - command receiver for this worker
            let cmd_recvr = self.worker_cmd_chan.new_recvr();

            // - report channel for this worker
            let (report_sender, report_recvr) = channel::<WorkerReport>();
            self.worker_report_recvrs.insert(dev_name.clone(), report_recvr);

            // - retrieve start_sync
            let worker_start_sync = start_sync.remove(&dev_name).unwrap();

            // Launch worker thread
            let stop_flag_clone = self.stop_flag.clone();
            let handle = thread::Builder::new()
                .name(dev_name.clone())
                .spawn(move || {
                    dev_container
                        .lock()
                        .worker_loop(chunksize_ms, cmd_recvr, report_sender, stop_flag_clone, worker_start_sync, target_rep_dur)
                })?;
            self.worker_handles.insert(dev_name, handle);
        }

        // Wait for all workers to report init completion.
        // If any worker had an error/panic, it will be visible as Err on trying to receive the report.
        for (name, report_recvr) in self.worker_report_recvrs.iter() {
            match report_recvr.recv().map_err(|_| format!("Worker {name} has quit"))? {
                WorkerReport::InitComplete => { /* worker reported successful init completion */ },
                other_unexpected => return Err(ManagerErr::new(format!("[BUG] Expected to receive `WorkerReport::InitComplete` but worker {name} reported {other_unexpected:?}")))
            }
        };
        Ok(())
    }

    pub fn run(&self, nreps: usize, main_requested_stop: &Arc<Mutex<bool>>, reps_written_count: &Arc<Mutex<usize>>) -> Result<(), ManagerErr> {
        // (1) Command all workers to start running
        *self.stop_flag.lock() = false;
        self.worker_cmd_chan.send(WorkerCmd::Run(nreps));
        let mut stopped_by_request = false;

        // (2) Keep checking on all workers throughout the run + react to stop request from `main` thread
        /*
           Every worker should report completion of each iteration.
           This is done to catch worker failures at runtime which will show up as `RecvErr` when trying to `recv()` the report.
        */
        for rep_idx in 0..nreps {
            for (name, report_recvr) in self.worker_report_recvrs.iter() {
                match report_recvr.recv().map_err(|_| format!("[Repetition {rep_idx}] worker {name} failed"))? {
                    WorkerReport::IterComplete => { /* Worker reported iteration completion as expected */ },
                    other_unexpected => return Err(ManagerErr::new(format!("[BUG] Expected to receive `WorkerReport::IterComplete` but worker {name} reported {other_unexpected:?}")))
                }
            }
            // Update "reps_written" counter
            *reps_written_count.lock() = rep_idx + 1;
            // Check if stop was requested by `main` thread
            if *main_requested_stop.lock() {
                *self.stop_flag.lock() = true;
                stopped_by_request = true;
                break
            }
        }

        // (3) Wait for every worker to report that it has completed "soft stop" and transitioned to the `Ready` state.
        // Any worker failing will show up as `RecvErr` when trying to `recv()` the report.
        /*
           Note:
            If run was interrupted by `main_requested_stop` there is an uncertainty in when each worker notices that `stop_flag` was raised.
            If some worker was "ahead", it might have checked for `stop_flag` and started another repetition before the flag was raised.
            As a result, different workers may complete different number of repetitions before stopping.
            This makes it necessary to have distinct variants `IterComplete` and `RunFinished` of `WorkerReport`
            and we need to keep "swallowing" `IterComplete`s until finally receiving `RunFinished`
        */
        for (name, report_recvr) in self.worker_report_recvrs.iter() {
            loop {
                match report_recvr.recv().map_err(|_| format!("[Collecting RunFinished reports] Worker {name} failed"))? {
                    WorkerReport::RunFinished => { break /* Worker reported completion as expected */ },
                    WorkerReport::IterComplete => {
                        if stopped_by_request {
                            // "Swallow" this extra `IterComplete` and continue waiting for `RunFinished`
                            continue
                        } else {
                            // This is unexpected - run was not stopped by external request,
                            // so every worker was expected to report `IterComplete` precisely `nreps` times
                            // which were already counted up in the previous `for`-loop
                            return Err(ManagerErr::new(format!("[BUG] worker {name} reported an unexpected extra `IterComplete`")))
                        }
                    },
                    other_unexpected => return Err(ManagerErr::new(format!("[BUG] Expected to receive `WorkerReport::RunFinished/IterComplete` but worker {name} reported {other_unexpected:?}")))
                }
            }
        }
        Ok(())
    }

    pub fn shut_down_all_workers(&mut self) -> Result<(), String> {
        // Command all workers to break out of any stream loops and return.
        *self.stop_flag.lock() = true;
        self.worker_cmd_chan.send(WorkerCmd::Close);
        // After these commands, every still-running worker will eventually return.
        // Plus any failed ones have quit already by either returning an Err or panicking.
        // So joining all handles should not lead to a deadlock.

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