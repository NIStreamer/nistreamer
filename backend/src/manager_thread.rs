use std::collections::HashMap;
use std::thread;
use std::thread::JoinHandle;
use std::sync::{Arc};
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
use indexmap::IndexMap;

use crate::device::{RunControl, NIDev, StartSync, WorkerReport, WorkerError};
use crate::worker_cmd_chan::{CmdChan, WorkerCmd};

pub enum ManagerCmd {
    Launch(usize),
    Close,
}

pub fn manager_loop(
    devs: IndexMap<String, Arc<Mutex<NIDev>>>,
    cmd_recvr: Receiver<ManagerCmd>,
    report_sender: Sender<()>,
    main_requested_stop: Arc<Mutex<bool>>,
    starts_last: Option<String>,
    bufsize_ms: f64,
) -> Result<(), String> {
    // Init
    let mut manager = StreamManager::new_uninit(main_requested_stop);

    // Convenience closure to simplify error handling
    // - using `?` operator will return immediately if an error occurs at any step.
    let catch_closure = || -> Result<(), String> {
        manager.try_init(devs, starts_last, bufsize_ms)?;
        report_sender.send(()).map_err(|err| err.to_string())?;

        loop {
            match cmd_recvr.recv().map_err(|err| err.to_string())? {
                ManagerCmd::Launch(nreps) => { /* ToDo */ }
                ManagerCmd::Close => {
                    break
                }
            }
        }

        Ok(())
    };

    // The actual manager loop logic:
    let closure_result = catch_closure();

    // Shut down all workers (no matter what main logic result was)
    let worker_results = manager.shut_down_all_workers();

    // Return
    if closure_result.is_ok() && worker_results.is_ok() {
        Ok(())
    } else {
        let mut joint_err_msg = String::new();
        if let Err(msg) = closure_result {
            joint_err_msg.push_str(&format!("\nManager error report: \n{msg}"))
        }
        if let Err(msg) = worker_results {
            joint_err_msg.push_str(&format!("\nWorker error reports: \n{msg}"))
        }
        Err(joint_err_msg)
    }
}

pub struct StreamManager {
    worker_handles: IndexMap<String, JoinHandle<Result<(), WorkerError>>>,
    worker_cmd_chan: CmdChan,  // command broadcasting channel
    worker_report_recvrs: IndexMap<String, Receiver<WorkerReport>>,
    stop_flag: Arc<Mutex<bool>>,
    main_requested_stop: Arc<Mutex<bool>>,
}

impl StreamManager {
    pub fn new_uninit(main_requested_stop: Arc<Mutex<bool>>) -> Self {
        Self {
            worker_handles: IndexMap::new(),
            worker_cmd_chan: CmdChan::new(),
            worker_report_recvrs: IndexMap::new(),
            stop_flag: Arc::new(Mutex::new(false)),
            main_requested_stop
        }
    }

    pub fn try_init(
        &mut self,
        mut devs: IndexMap<String, Arc<Mutex<NIDev>>>,
        starts_last: Option<String>,
        bufsize_ms: f64,
    ) -> Result<(), String> {
        // - inter-worker start sync channels
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
            let spawn_result = thread::Builder::new()
                .name(dev_name.clone())
                .spawn(move || {
                    let mut typed_dev = dev_container.lock();
                    match &mut *typed_dev {
                        NIDev::AO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sender, stop_flag_clone, worker_start_sync, target_rep_dur),
                        NIDev::DO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sender, stop_flag_clone, worker_start_sync, target_rep_dur),
                    }
                });
            match spawn_result {
                Ok(handle) => { self.worker_handles.insert(dev_name.to_string(), handle); },
                Err(err) => {
                    /* OS failed to launch this thread */
                    return Err(err.to_string())
                },
            };
        }

        // Wait for all workers to report init completion.
        // If any worker had an error/panic, it will be visible as Err on trying to receive the report.
        for (name, report_recvr) in self.worker_report_recvrs.iter() {
            match report_recvr.recv().map_err(|recv_err| recv_err.to_string())? {
                WorkerReport::InitComplete => { },
                other_unexpected => return Err(format!("[BUG] Expected to receive `WorkerReport::InitComplete` but worker {name} reported {other_unexpected:?}"))
            }
        };

        Ok(())
    }

    /*
    fn collect_worker_reports(&self) -> Result<(), ()> {
        // Wait for each worker thread to report completion or stop working (by returning or panicking)
        for (dev_name, recvr) in self.worker_report_recvrs.iter() {
            match recvr.recv() {
                Ok(()) => {},
                Err(_err) => return Err(()),
            };
        };
        Ok(())
    }
    */
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
                joint_err_msg.push_str(&format!("[{name}] {msg}\n"))
            }
            Err(joint_err_msg)
        }
    }
}