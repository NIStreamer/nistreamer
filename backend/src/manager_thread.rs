use std::collections::HashMap;
use std::thread;
use std::thread::JoinHandle;
use std::sync::{Arc};
use std::sync::mpsc::{channel, Sender, Receiver};
use parking_lot::Mutex;
use indexmap::IndexMap;

use crate::device::{RunControl, NIDev, StartSync, WorkerError};
use crate::worker_cmd_chan::{CmdChan, WorkerCmd};

pub enum ManagerCmd {
    Launch(usize),
    Close,
}

pub fn manager_loop(
    devs: IndexMap<String, Arc<Mutex<NIDev>>>,
    cmd_recvr: Receiver<ManagerCmd>,
    report_sender: Sender<Result<(), String>>,
    main_requested_stop: Arc<Mutex<bool>>,
    starts_last: Option<String>,
    bufsize_ms: f64,
) {
    // Init
    let init_res = StreamManager::try_init(
        devs,
        main_requested_stop,
        starts_last,
        bufsize_ms,
    );
    let mut manager = match init_res {
        Ok(manager) => {
            report_sender.send(Ok(()));
            manager
        },
        Err(msg) => {
            report_sender.send(Err(msg));
            return ()
        }
    };

    loop {
        match cmd_recvr.recv().unwrap() {
            ManagerCmd::Launch(nreps) => {}
            ManagerCmd::Close => {
                // ToDo: Signal all workers to quit, join worker threads
                let _ = report_sender.send(Ok(()));
                break
            }
        }
    }
}

pub struct StreamManager {
    worker_handles: IndexMap<String, JoinHandle<Result<(), WorkerError>>>,
    worker_cmd_chan: CmdChan,  // command broadcasting channel
    internal_stop_flag: Arc<Mutex<bool>>,
    worker_report_recvrs: IndexMap<String, Receiver<()>>,
    main_stop_flag: Arc<Mutex<bool>>,
}

impl StreamManager {
    pub fn try_init(
        mut devs: IndexMap<String, Arc<Mutex<NIDev>>>,
        main_stop_flag: Arc<Mutex<bool>>,
        starts_last: Option<String>,
        bufsize_ms: f64,
    ) -> Result<Self, String> {
        let mut manager = Self {
            worker_handles: IndexMap::new(),
            worker_cmd_chan: CmdChan::new(),
            internal_stop_flag: Arc::new(Mutex::new(false)),
            worker_report_recvrs: IndexMap::new(),
            main_stop_flag
        };

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
        let mut worker_err_log = IndexMap::new();
        for (dev_name, dev_container) in devs.drain(..) {
            // - command receiver for this worker
            let cmd_recvr = manager.worker_cmd_chan.new_recvr();

            // - report channel for this worker
            let (report_sendr, report_recvr) = channel::<()>();
            manager.worker_report_recvrs.insert(dev_name.clone(), report_recvr);

            // - retrieve start_sync
            let worker_start_sync = start_sync.remove(&dev_name).unwrap();

            // Launch worker thread
            let spawn_result = thread::Builder::new()
                .name(dev_name.clone())
                .spawn(move || {
                    let mut typed_dev = dev_container.lock();
                    match &mut *typed_dev {
                        NIDev::AO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sendr, worker_start_sync, target_rep_dur),
                        NIDev::DO(dev) => dev.worker_loop(bufsize_ms, cmd_recvr, report_sendr, worker_start_sync, target_rep_dur),
                    }
                });
            match spawn_result {
                Ok(handle) => { manager.worker_handles.insert(dev_name.to_string(), handle); },
                Err(err) => {
                    /* OS failed to launch this thread */
                    worker_err_log.insert(dev_name.clone(), err.to_string());
                    // Dispose of report receiver for this failed worker right away
                    let _ = manager.worker_report_recvrs.shift_remove(&dev_name);
                    break
                },
            };
        }

        // Wait for workers to report init completion or failure
        let overall_worker_report = manager.collect_worker_reports();

        // Close all remaining workers and return the error if any of the workers has failed
        if !worker_err_log.is_empty() || overall_worker_report.is_err() {
            // At least one worker has failed to launch / init. Command all remaining ones to quit,
            // and join all worker threads, and collect error messages from all failed ones

            manager.worker_cmd_chan.send(WorkerCmd::Close);

            for (worker_name, worker_handle) in manager.worker_handles.drain(..) {
                match worker_handle.join() {
                    Ok(worker_res) => match worker_res {
                        Ok(()) => {},
                        Err(worker_err) => { worker_err_log.insert(worker_name, worker_err.to_string()); },
                    },
                    Err(panic_content) => { worker_err_log.insert(worker_name, format!("{panic_content:?}")); },
                }
            };

            let mut joint_err_msg = String::new();
            for (dev_name, err) in worker_err_log {
                joint_err_msg.push_str(&format!("[{dev_name}] {err}"))
            }
            return Err(joint_err_msg)
        };

        Ok(manager)
    }

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
}