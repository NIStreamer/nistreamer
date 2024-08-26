use pyo3::prelude::*;
use pyo3::exceptions::{PyValueError, PyKeyError, PyRuntimeError};

use base_streamer::channel::BaseChan;
use base_streamer::device::BaseDev;
use base_streamer::streamer::BaseStreamer;
use base_streamer::fn_lib_tools::{FnBoxF64, FnBoxBool};

use crate::channel::{AOChan, DOChan};
use crate::device::{AODev, CommonHwCfg, DODev, NIDev};
use crate::streamer::Streamer;

#[pyclass]
pub struct StreamerWrap {
    inner: Streamer
}

#[pymethods]
impl StreamerWrap {
    #[new]
    pub fn new() -> Self {
        Self {
            inner: Streamer::new()
        }
    }

    pub fn add_ao_dev(&mut self, name: &str, samp_rate: f64) -> PyResult<()> {
        let dev = AODev::new(name, samp_rate);
        match self.inner.add_ao_dev(dev) {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyValueError::new_err(msg)),
        }
    }

    pub fn add_do_dev(&mut self, name: &str, samp_rate: f64) -> PyResult<()> {
        let dev = DODev::new(name, samp_rate);
        match self.inner.add_do_dev(dev) {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyValueError::new_err(msg)),
        }
    }

    // region Hardware settings
    pub fn get_starts_last(&self) -> Option<String> {
        self.inner.get_starts_last()
    }
    #[pyo3(signature = (name))]
    pub fn set_starts_last(&mut self, name: Option<String>) {
        self.inner.set_starts_last(name)
    }

    pub fn get_ref_clk_provider(&self) -> Option<(String, String)> {
        self.inner.get_ref_clk_provider()
    }
    #[pyo3(signature = (provider))]
    pub fn set_ref_clk_provider(&mut self, provider: Option<(String, String)>) {
        self.inner.set_ref_clk_provider(provider);
    }

    pub fn reset_all(&self) -> PyResult<()> {
        match self.inner.reset_all() {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyRuntimeError::new_err(msg)),
        }
    }
    // endregion

    // region Compile
    fn last_instr_end_time(&self) -> f64 {
        self.inner.last_instr_end_time()
    }

    fn total_run_time(&self) -> f64 {
        self.inner.total_run_time()
    }

    #[pyo3(signature = (stop_time=None))]
    fn compile(&mut self, stop_time: Option<f64>) -> PyResult<f64> {
        match self.inner.compile(stop_time) {
            Ok(total_run_time) => Ok(total_run_time),
            Err(msg) => Err(PyValueError::new_err(msg)),
        }
    }

    fn is_fresh_compiled(&self) -> bool {
        self.inner.is_fresh_compiled()
    }

    fn clear_edit_cache(&mut self) {
        self.inner.clear_edit_cache()
    }

    #[pyo3(signature = (reset_time=None))]
    fn add_reset_instr(&mut self, reset_time: Option<f64>) -> PyResult<()> {
        match self.inner.add_reset_instr(reset_time) {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyValueError::new_err(msg))
        }
    }
    // endregion

    // region Run control
    pub fn cfg_run(&mut self, bufsize_ms: f64) -> PyResult<()> {
        match self.inner.cfg_run_(bufsize_ms) {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyValueError::new_err(msg)),
        }
    }

    pub fn stream_run(&mut self, calc_next: bool) -> PyResult<()> {
        match self.inner.stream_run_(calc_next) {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyRuntimeError::new_err(msg)),
        }
    }

    pub fn close_run(&mut self) -> PyResult<()> {
        match self.inner.close_run_() {
            Ok(()) => Ok(()),
            Err(msg) => Err(PyRuntimeError::new_err(msg)),
        }
    }
    // endregion
}

// region Device methods
impl StreamerWrap {
    fn assert_has_dev(&self, dev_name: &str) -> PyResult<()> {
        if self.inner.devs().contains_key(dev_name) {
            Ok(())
        } else {
            Err(PyKeyError::new_err(format!(
                "There is no device with name {dev_name} registered.\n\
                The following device names are registered: {:?}",
                self.inner.devs().keys()
            )))
        }
    }

    pub fn get_dev(&self, dev_name: &str) -> PyResult<&NIDev> {
        self.assert_has_dev(dev_name)?;
        Ok(self.inner.devs().get(dev_name).unwrap())
    }

    pub fn get_dev_mut(&mut self, dev_name: &str) -> PyResult<&mut NIDev> {
        self.assert_has_dev(dev_name)?;
        Ok(self.inner.devs_mut().get_mut(dev_name).unwrap())
    }
}

#[pymethods]
impl StreamerWrap {
    pub fn add_ao_chan(&mut self, dev_name: &str, chan_idx: usize, dflt_val: f64, rst_val: f64) -> PyResult<()> {
        let typed_dev = self.get_dev_mut(dev_name)?;

        if let NIDev::AO(dev) = typed_dev {
            let chan = AOChan::new(chan_idx, dev.samp_rate(), dflt_val, rst_val);
            match dev.add_chan_sort(chan) {
                Ok(()) => Ok(()),
                Err(msg) => Err(PyKeyError::new_err(msg)),
            }
        } else {
            Err(PyKeyError::new_err(format!("Cannot add analog output channel to non-AO device {dev_name}")))
        }
    }

    pub fn add_do_chan(&mut self, dev_name: &str, port_idx: usize, line_idx: usize, dflt_val: bool, rst_val: bool) -> PyResult<()> {
        let typed_dev = self.get_dev_mut(dev_name)?;

        if let NIDev::DO(dev) = typed_dev {
            let chan = DOChan::new(port_idx, line_idx, dev.samp_rate(), dflt_val, rst_val);
            match dev.add_chan_sort(chan) {
                Ok(()) => Ok(()),
                Err(msg) => Err(PyKeyError::new_err(msg)),
            }
        } else {
            Err(PyKeyError::new_err(format!("Cannot add digital output channel to non-DO device {dev_name}")))
        }
    }

    pub fn dev_clear_edit_cache(&mut self, dev_name: &str) -> PyResult<()> {
        let typed_dev = self.get_dev_mut(dev_name)?;
        match typed_dev {
            NIDev::AO(dev) => dev.clear_edit_cache(),
            NIDev::DO(dev) => dev.clear_edit_cache(),
        };
        Ok(())
    }

    // region Hardware settings
    pub fn dev_get_samp_rate(&self, dev_name: &str) -> PyResult<f64> {
        let typed_dev = self.get_dev(dev_name)?;
        let samp_rate = match typed_dev {
            NIDev::AO(dev) => dev.samp_rate(),
            NIDev::DO(dev) => dev.samp_rate(),
        };
        Ok(samp_rate)
    }

    pub fn dev_get_start_trig_in(&self, dev_name: &str) -> PyResult<Option<String>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().start_trig_in.clone())
    }

    #[pyo3(signature = (dev_name, term))]
    pub fn dev_set_start_trig_in(&mut self, dev_name: &str, term: Option<String>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().start_trig_in = term;
        Ok(())
    }

    pub fn dev_get_start_trig_out(&self, dev_name: &str) -> PyResult<Option<String>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().start_trig_out.clone())
    }

    #[pyo3(signature = (dev_name, term))]
    pub fn dev_set_start_trig_out(&mut self, dev_name: &str, term: Option<String>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().start_trig_out = term;
        Ok(())
    }

    pub fn dev_get_samp_clk_in(&self, dev_name: &str) -> PyResult<Option<String>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().samp_clk_in.clone())
    }

    #[pyo3(signature = (dev_name, term))]
    pub fn dev_set_samp_clk_in(&mut self, dev_name: &str, term: Option<String>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().samp_clk_in = term;
        Ok(())
    }

    pub fn dev_get_samp_clk_out(&self, dev_name: &str) -> PyResult<Option<String>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().samp_clk_out.clone())
    }

    #[pyo3(signature = (dev_name, term))]
    pub fn dev_set_samp_clk_out(&mut self, dev_name: &str, term: Option<String>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().samp_clk_out = term;
        Ok(())
    }

    pub fn dev_get_ref_clk_in(&self, dev_name: &str) -> PyResult<Option<String>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().ref_clk_in.clone())
    }

    #[pyo3(signature = (dev_name, term))]
    pub fn dev_set_ref_clk_in(&mut self, dev_name: &str, term: Option<String>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().ref_clk_in = term;
        Ok(())
    }

    pub fn dev_get_min_bufwrite_timeout(&self, dev_name: &str) -> PyResult<Option<f64>> {
        let ni_dev = self.get_dev(dev_name)?;
        Ok(ni_dev.hw_cfg().min_bufwrite_timeout.clone())
    }

    #[pyo3(signature = (dev_name, min_timeout))]
    pub fn dev_set_min_bufwrite_timeout(&mut self, dev_name: &str, min_timeout: Option<f64>) -> PyResult<()> {
        let ni_dev = self.get_dev_mut(dev_name)?;
        ni_dev.hw_cfg_mut().min_bufwrite_timeout = min_timeout;
        Ok(())
    }
    // endregion
}
// endregion

// region Channel methods
impl StreamerWrap {
    fn get_ao_chan(&self, dev_name: &str, chan_idx: usize) -> PyResult<&AOChan> {
        let typed_dev = self.get_dev(dev_name)?;

        if let NIDev::AO(dev) = typed_dev {
            let chan_name = format!("ao{chan_idx}");

            if let Some(chan) = dev.chans().get(&chan_name) {
                Ok(chan)
            } else {
                Err(PyKeyError::new_err(format!(
                    "AO device {dev_name} does not have a channel {chan_name} registered"
                )))
            }
        } else {
            Err(PyKeyError::new_err(format!(
                "Device {dev_name} is not an AO device and cannot have AO channels"
            )))
        }
    }

    fn get_do_chan(&self, dev_name: &str, port: usize, line: usize) -> PyResult<&DOChan> {
        let typed_dev = self.get_dev(dev_name)?;

        if let NIDev::DO(dev) = typed_dev {
            let chan_name = format!("port{port}/line{line}");

            if let Some(chan) = dev.chans().get(&chan_name) {
                Ok(chan)
            } else {
                Err(PyKeyError::new_err(format!(
                    "DO device {dev_name} does not have a channel {chan_name} registered"
                )))
            }
        } else {
            Err(PyKeyError::new_err(format!(
                "Device {dev_name} is not a DO device and cannot have DO channels"
            )))
        }
    }

    fn get_ao_chan_mut(&mut self, dev_name: &str, chan_idx: usize) -> PyResult<&mut AOChan> {
        let typed_dev = self.get_dev_mut(dev_name)?;

        if let NIDev::AO(dev) = typed_dev {
            let chan_name = format!("ao{chan_idx}");

            if let Some(chan) = dev.chans_mut().get_mut(&chan_name) {
                Ok(chan)
            } else {
                Err(PyKeyError::new_err(format!(
                    "AO device {dev_name} does not have a channel {chan_name} registered"
                )))
            }
        } else {
            Err(PyKeyError::new_err(format!(
                "Device {dev_name} is not an AO device and cannot have AO channels"
            )))
        }
    }

    fn get_do_chan_mut(&mut self, dev_name: &str, port: usize, line: usize) -> PyResult<&mut DOChan> {
        let typed_dev = self.get_dev_mut(dev_name)?;

        if let NIDev::DO(dev) = typed_dev {
            let chan_name = format!("port{port}/line{line}");

            if let Some(chan) = dev.chans_mut().get_mut(&chan_name) {
                Ok(chan)
            } else {
                Err(PyKeyError::new_err(format!(
                    "DO device {dev_name} does not have a channel {chan_name} registered"
                )))
            }
        } else {
            Err(PyKeyError::new_err(format!(
                "Device {dev_name} is not a DO device and cannot have DO channels"
            )))
        }
    }
}

#[pymethods]
impl StreamerWrap {
    pub fn ao_chan_dflt_val(&self, dev_name: &str, chan_idx: usize) -> PyResult<f64> {
        let chan = self.get_ao_chan(dev_name, chan_idx)?;
        Ok(chan.dflt_val())
    }

    pub fn do_chan_dflt_val(&self, dev_name: &str, port: usize, line: usize) -> PyResult<bool> {
        let chan = self.get_do_chan(dev_name, port, line)?;
        Ok(chan.dflt_val())
    }

    pub fn ao_chan_last_instr_end_time(&self, dev_name: &str, chan_idx: usize) -> PyResult<f64> {
        let chan = self.get_ao_chan(dev_name, chan_idx)?;
        Ok(chan.last_instr_end_time())
    }

    pub fn do_chan_last_instr_end_time(&self, dev_name: &str, port: usize, line: usize) -> PyResult<f64> {
        let chan = self.get_do_chan(dev_name, port, line)?;
        Ok(chan.last_instr_end_time())
    }

    pub fn ao_chan_clear_edit_cache(&mut self, dev_name: &str, chan_idx: usize) -> PyResult<()> {
        let chan = self.get_ao_chan_mut(dev_name, chan_idx)?;
        chan.clear_edit_cache();
        Ok(())
    }

    pub fn do_chan_clear_edit_cache(&mut self, dev_name: &str, port: usize, line: usize) -> PyResult<()> {
        let chan = self.get_do_chan_mut(dev_name, port, line)?;
        chan.clear_edit_cache();
        Ok(())
    }

    #[pyo3(signature = (dev_name, chan_idx, func, t, dur_spec))]
    pub fn ao_chan_add_instr(
        &mut self, dev_name: &str, chan_idx: usize,
        func: FnBoxF64, t: f64, dur_spec: Option<(f64, bool)>
    ) -> PyResult<()> {
        let chan = self.get_ao_chan_mut(dev_name, chan_idx)?;
        chan.add_instr(func.inner, t, dur_spec);
        Ok(())
    }

    #[pyo3(signature = (dev_name, port, line, func, t, dur_spec))]
    pub fn do_chan_add_instr(
        &mut self, dev_name: &str, port: usize, line: usize,
        func: FnBoxBool, t: f64, dur_spec: Option<(f64, bool)>
    ) -> PyResult<()> {
        let chan = self.get_do_chan_mut(dev_name, port, line)?;
        chan.add_instr(func.inner, t, dur_spec);
        Ok(())
    }

    pub fn ao_chan_calc_nsamps(
        &self,
        dev_name: &str, chan_idx: usize,
        start_time: f64, end_time: f64, n_samps: usize
    ) -> PyResult<Vec<f64>> {
        let chan = self.get_ao_chan(dev_name, chan_idx)?;
        Ok(chan.calc_signal_nsamps(start_time, end_time, n_samps))
    }

    pub fn do_chan_calc_nsamps(
        &self,
        dev_name: &str, port: usize, line: usize,
        start_time: f64, end_time: f64, n_samps: usize
    ) -> PyResult<Vec<bool>> {
        let chan = self.get_do_chan(dev_name, port, line)?;
        Ok(chan.calc_signal_nsamps(start_time, end_time, n_samps))
    }

    /*

    pub fn chan_add_instr(&mut self, dev_name: &str, chan_name: &str, t: f64, end_spec: Option<(f64, bool)>, func: /*ToDo*/ f64) {
        todo!()
    }

    pub fn channel_clear_edit_cache(&mut self, dev_name: &str, chan_name: &str)

    pub fn channel_last_instr_end_time(&mut self, dev_name: &str, chan_name: &str) -> f64

    pub fn channel_calc_signal_nsamps(
        &mut self,
        dev_name: &str,
        chan_name: &str,
        start_time: f64,
        end_time: f64,
        num_samps: usize,
    ) -> Vec<f64> {
        todo!()
    } */
}
// endregion