use crate::device::BaseDev;

/// Type-agnostic ("Tag") `BaseDevice` trait - set of methods which are not aware of the device's
/// actual sample or channel types. `BaseStreamer` trait is only using these methods allowing for
/// devices of different types being treated uniformly - as `dyn TagBaseDev` trait objects.
pub trait TagBaseDev {
    fn tag_name(&self) -> String;
    fn tag_samp_rate(&self) -> f64;
    fn tag_got_instructions(&self) -> bool;
    fn tag_last_instr_end_time(&self) -> Option<f64>;
    fn tag_compile(&mut self, stop_time: f64) -> Result<(), String>;
    fn tag_clear_edit_cache(&mut self);
    fn tag_clear_compile_cache(&mut self);
    fn tag_validate_compile_cache(&self) -> Result<(), String>;
    fn tag_compiled_stop_time(&self) -> f64;
    fn tag_add_reset_instr(&mut self, reset_time: f64) -> Result<(), String>;
}

impl<D: BaseDev> TagBaseDev for D {
    fn tag_name(&self) -> String {
        self.name()
    }

    fn tag_samp_rate(&self) -> f64 {
        self.samp_rate()
    }

    fn tag_got_instructions(&self) -> bool {
        self.got_instructions()
    }

    fn tag_last_instr_end_time(&self) -> Option<f64> {
        self.last_instr_end_time()
    }

    fn tag_compile(&mut self, stop_time: f64) -> Result<(), String> {
        self.compile(stop_time)
    }

    fn tag_clear_edit_cache(&mut self) {
        self.clear_edit_cache()
    }

    fn tag_clear_compile_cache(&mut self) {
        self.clear_compile_cache()
    }

    fn tag_validate_compile_cache(&self) -> Result<(), String> {
        self.validate_compile_cache()
    }

    fn tag_compiled_stop_time(&self) -> f64 {
        self.compiled_stop_time()
    }

    fn tag_add_reset_instr(&mut self, reset_time: f64) -> Result<(), String> {
        self.add_reset_instr(reset_time)
    }
}

pub trait BaseStreamer {
    fn devs(&self) -> Vec<&dyn TagBaseDev>;
    fn devs_mut(&mut self) -> Vec<&mut dyn TagBaseDev>;

    fn check_can_add_dev(&self, name: String) -> Result<(), String> {
        let dev_names: Vec<_> = self.devs().iter().map(|dev| dev.tag_name()).collect();
        if dev_names.contains(&name) {
            return Err(format!("There is already a device with name {name} registered. Registered devices are {dev_names:?}"))
        };
        Ok(())
    }

    fn last_instr_end_time(&self) -> Option<f64> {
        self.devs()
            .iter()
            .filter_map(|dev| dev.tag_last_instr_end_time())
            .reduce(|largest_so_far, this| f64::max(largest_so_far, this))
    }

    fn got_instructions(&self) -> bool {
        self.devs()
            .iter()
            .any(|dev| dev.tag_got_instructions())
    }

    fn active_devs(&self) -> Vec<&dyn TagBaseDev> {
        self.devs()
            .drain(..)
            .filter(|dev| dev.tag_got_instructions())
            .collect()
    }

    fn active_devs_mut(&mut self) -> Vec<&mut dyn TagBaseDev> {
        self.devs_mut()
            .drain(..)
            .filter(|dev| dev.tag_got_instructions())
            .collect()
    }

    fn active_dev_names(&self) -> Vec<String> {
        self.active_devs()
            .iter()
            .map(|dev| dev.tag_name())
            .collect()
    }

    fn compile(&mut self, stop_time: Option<f64>) -> Result<f64, String> {
        if !self.got_instructions() {
            return Err(format!("Streamer did not get any instructions"))
        }
        let stop_time = match stop_time {
            Some(stop_time) => {
                if stop_time < self.last_instr_end_time().unwrap() {
                    return Err(format!(
                        "Attempted to compile with stop_time={stop_time} [s] while the last instruction end time is {} [s]\n\
                        If you intended to provide stop_time=last_instr_end_time, use stop_time=None",
                        self.last_instr_end_time().unwrap()
                    ))
                };
                stop_time
            },
            None => self.last_instr_end_time().unwrap(),
        };

        for dev in self.active_devs_mut() {
            dev.tag_compile(stop_time)?;
        }

        Ok(self.shortest_dev_run_time())
    }

    fn clear_compile_cache(&mut self) {
        for dev in self.devs_mut() {
            dev.tag_clear_compile_cache()
        }
    }

    fn clear_edit_cache(&mut self) {
        for dev in self.devs_mut() {
            dev.tag_clear_edit_cache()
        };
        self.clear_compile_cache();
    }

    fn validate_compile_cache(&self) -> Result<(), String> {
        // 2 checks:
        // - streamer got instructions in the first place;
        // - all active devices pass compile cache validation
        /* [Unlike in `BaseDev::validate_compule_cache()`, here we don't check that all devices
           were compiled to the same stop time. It would be nice, but is hard since devices run on
           different sample clocks and may have extra ticks due to closing edge clipping so they
           will naturally stop at slightly different times even when asked to compile to the same one]*/

        if !self.got_instructions() {
            return Err(format!("Streamer did not get any instructions"))
        }

        let failed_dev_msgs: Vec<String> = self
            .active_devs()
            .iter()
            .map(|dev| dev.tag_validate_compile_cache())
            .filter_map(|res| res.err())
            .collect();
        if !failed_dev_msgs.is_empty() {
            let mut full_err_msg = String::new();
            for msg in failed_dev_msgs {
                full_err_msg.push_str(&format!("{msg}\n"))
            };
            return Err(format!("The following devices failed compile cache validation:\n{full_err_msg}"))
        }

        Ok(())
    }

    fn shortest_dev_run_time(&self) -> f64 {
        // Sanity checks:
        /* @Backend developers: before trying to access compile cache
           you should always ensure the streamer actually got some instructions
           and that compile cache is valid (up-to-date with the current edit cache).
           Compile cache typically gets invalid due to users forgetting to re-compile after adding pulses.

           Functions `got_instructions()` and `validate_compile_cache()` are meant to be the place
           to do these checks gracefully. Other functions typically assume these checks have been done.
           They likely still double check but may just panic if the tests fail like in the example below.
        */
        if !self.got_instructions() {
            panic!("Streamer did not get any instructions")
        }
        self.validate_compile_cache().unwrap();

        self.active_devs()
            .iter()
            .map(|dev| dev.tag_compiled_stop_time())
            .reduce(|shortest_so_far, this| f64::min(shortest_so_far, this))
            .unwrap()
    }

    fn longest_dev_run_time(&self) -> f64 {
        // Sanity checks:
        /* @Backend developers: before trying to access compile cache
           you should always ensure the streamer actually got some instructions
           and that compile cache is valid (up-to-date with the current edit cache).
           Compile cache typically gets invalid due to users forgetting to re-compile after adding pulses.

           Functions `got_instructions()` and `validate_compile_cache()` are meant to be the place
           to do these checks gracefully. Other functions typically assume these checks have been done.
           They likely still double check but may just panic if the tests fail like in the example below.
        */
        if !self.got_instructions() {
            panic!("Streamer did not get any instructions")
        }
        self.validate_compile_cache().unwrap();

        self.active_devs()
            .iter()
            .map(|dev| dev.tag_compiled_stop_time())
            .reduce(|longest_so_far, this| f64::max(longest_so_far, this))
            .unwrap()
    }

    fn add_reset_instr(&mut self, reset_time: Option<f64>) -> Result<(), String> {
        let reset_time = match reset_time {
            Some(reset_time) => {
                if self.last_instr_end_time().is_some_and(|last_instr_end| reset_time < last_instr_end){
                    return Err(format!(
                        "Requested to insert the all-channel reset instruction at t = {reset_time} [s] \
                        but some channels have instructions spanning until {} [s].\n\
                        If you intended to provide `reset_time=last_instr_end_time`, use `reset_time=None`",
                        self.last_instr_end_time().unwrap()
                    ))
                }
                reset_time
            },
            None => self.last_instr_end_time().unwrap_or(0.0),
        };
        for dev in self.devs_mut() {
            dev.tag_add_reset_instr(reset_time)?
        };
        Ok(())
    }
}