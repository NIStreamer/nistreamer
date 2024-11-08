use std::collections::BTreeSet;

use base_streamer::fn_lib_tools::FnTraitSet;
use base_streamer::instruction::Instr;
use base_streamer::channel::BaseChan;

// region AO Channel
pub struct AOChan {
    idx: usize,
    samp_rate: f64,
    dflt_val: f64,
    rst_val: f64,
    is_fresh_compiled: bool,
    instr_list: BTreeSet<Instr<f64>>,
    compile_cache_ends: Vec<usize>,
    compile_cache_fns: Vec<Box<dyn FnTraitSet<f64>>>
}

impl AOChan {
    pub fn new(idx: usize, samp_rate: f64, dflt_val: f64, rst_val: f64) -> Self {
        Self {
            idx,
            samp_rate,
            dflt_val,
            rst_val,
            is_fresh_compiled: true,
            instr_list: BTreeSet::new(),
            compile_cache_ends: Vec::new(),
            compile_cache_fns: Vec::new(),
        }
    }

    pub fn idx(&self) -> usize {
        self.idx
    }
}

impl BaseChan<f64> for AOChan {
    fn name(&self) -> String {
        format!("ao{}", self.idx)
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
    }

    fn is_fresh_compiled(&self) -> bool {
        self.is_fresh_compiled
    }

    fn dflt_val(&self) -> f64 {
        self.dflt_val
    }

    fn rst_val(&self) -> f64 {
        self.rst_val
    }

    fn instr_list(&self) -> &BTreeSet<Instr<f64>> {
        &self.instr_list
    }

    fn compile_cache_ends(&self) -> &Vec<usize> {
        &self.compile_cache_ends
    }

    fn compile_cache_fns(&self) -> &Vec<Box<dyn FnTraitSet<f64>>> {
        &self.compile_cache_fns
    }

    fn fresh_compiled_mut(&mut self) -> &mut bool {
        &mut self.is_fresh_compiled
    }

    fn instr_list_mut(&mut self) -> &mut BTreeSet<Instr<f64>> {
        &mut self.instr_list
    }

    fn compile_cache_ends_mut(&mut self) -> &mut Vec<usize> {
        &mut self.compile_cache_ends
    }

    fn compile_cache_fns_mut(&mut self) -> &mut Vec<Box<dyn FnTraitSet<f64>>> {
        &mut self.compile_cache_fns
    }
}
// endregion

// region DO Channel
pub struct DOChan {
    port: usize,
    line: usize,
    samp_rate: f64,
    dflt_val: bool,
    rst_val: bool,
    is_fresh_compiled: bool,
    instr_list: BTreeSet<Instr<bool>>,
    compile_cache_ends: Vec<usize>,
    compile_cache_fns: Vec<Box<dyn FnTraitSet<bool>>>
}

impl DOChan {
    pub fn new(port: usize, line: usize, samp_rate: f64, dflt_val: bool, rst_val: bool) -> Self {
        Self {
            port,
            line,
            samp_rate,
            dflt_val,
            rst_val,
            is_fresh_compiled: false,
            instr_list: BTreeSet::new(),
            compile_cache_ends: Vec::new(),
            compile_cache_fns: Vec::new(),
        }
    }

    pub fn port(&self) -> usize {
        self.port
    }

    pub fn line(&self) -> usize {
        self.line
    }
}

impl BaseChan<bool> for DOChan {
    fn name(&self) -> String {
        format!("port{}/line{}", self.port, self.line)
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
    }

    fn is_fresh_compiled(&self) -> bool {
        self.is_fresh_compiled
    }

    fn dflt_val(&self) -> bool {
        self.dflt_val
    }

    fn rst_val(&self) -> bool {
        self.rst_val
    }

    fn instr_list(&self) -> &BTreeSet<Instr<bool>> {
        &self.instr_list
    }

    fn compile_cache_ends(&self) -> &Vec<usize> {
        &self.compile_cache_ends
    }

    fn compile_cache_fns(&self) -> &Vec<Box<dyn FnTraitSet<bool>>> {
        &self.compile_cache_fns
    }

    fn fresh_compiled_mut(&mut self) -> &mut bool {
        &mut self.is_fresh_compiled
    }

    fn instr_list_mut(&mut self) -> &mut BTreeSet<Instr<bool>> {
        &mut self.instr_list
    }

    fn compile_cache_ends_mut(&mut self) -> &mut Vec<usize> {
        &mut self.compile_cache_ends
    }

    fn compile_cache_fns_mut(&mut self) -> &mut Vec<Box<dyn FnTraitSet<bool>>> {
        &mut self.compile_cache_fns
    }
}

pub struct DOPort {
    pub idx: usize,
    pub ends: Vec<usize>,
    pub vals: Vec<u32>,
}

// region DO Port channel
impl DOPort {
    pub fn total_samps(&self) -> usize {
        match self.ends.last() {
            Some(&instr_end) => instr_end,
            None => 0,
        }
    }
}

impl DOPort {
    pub fn calc_samps(&self, window_start: usize, samp_buf: &mut [u32]) -> Result<(), String> {
        // Sanity check:
        let window_end = window_start + samp_buf.len();
        if !(window_end <= self.total_samps()) {
            return Err(format!("DOPort::calc_samps() sampling window end {window_end} goes beyond the total compiled sample number {}", self.total_samps()))
        }

        // Find all instructions covered (fully or partially) by this window
        let first_instr_idx = match self.ends.binary_search(&window_start) {
            Ok(idx) => idx + 1,
            Err(idx) => idx,
        };
        let last_instr_idx = match self.ends.binary_search(&window_end) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };

        // Helper to map "absolute" clock grid position onto the index within sampling window
        let rm_offs = |pos| -> usize {pos - window_start};

        let mut cur_pos = window_start;
        for idx in first_instr_idx..=last_instr_idx {
            let &instr_end = self.ends.get(idx).unwrap();
            let &instr_val = self.vals.get(idx).unwrap();

            let next_pos = std::cmp::min(instr_end, window_end);

            let buf_slice = &mut samp_buf[rm_offs(cur_pos)..rm_offs(next_pos)];
            buf_slice.fill(instr_val);

            cur_pos = next_pos;
        }
       Ok(())
    }
}
// endregion
// endregion