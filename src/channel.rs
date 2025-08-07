use std::collections::BTreeSet;

use nistreamer_base::fn_lib_tools::FnTraitSet;
use nistreamer_base::instruction::Instr;
use nistreamer_base::channel::BaseChan;

// region AO Channel
pub struct AOChan {
    idx: usize,
    samp_rate: f64,
    dflt_val: f64,
    rst_val: f64,
    instr_list: BTreeSet<Instr<f64>>,
    compile_cache_ends: Vec<usize>,
    compile_cache_fns: Vec<Box<dyn FnTraitSet<f64>>>,
    is_fresh_compiled: bool
}

impl AOChan {
    pub fn new(idx: usize, samp_rate: f64, dflt_val: f64, rst_val: f64) -> Self {
        Self {
            idx,
            samp_rate,
            dflt_val,
            rst_val,
            instr_list: BTreeSet::new(),
            compile_cache_ends: Vec::new(),
            compile_cache_fns: Vec::new(),
            is_fresh_compiled: true,
        }
    }

    pub fn idx(&self) -> usize {
        self.idx
    }

    /// Generates full channel name string based on channel index - the single place where
    /// the naming format of AO channels is written out explicitly.
    ///
    /// Use this function wherever channel name string is required instead of assembling it
    /// manually with `format!(...)`. This will ensure everything automatically stays consistent
    /// throughout the project even if naming format has to be changed.
    pub fn name_fmt(idx: usize) -> String {
        format!("ao{idx}")
    }
}

impl BaseChan for AOChan {
    type Samp = f64;

    fn name(&self) -> String {
        AOChan::name_fmt(self.idx)
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
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

    fn is_fresh_compiled(&self) -> bool {
        self.is_fresh_compiled
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

    fn is_fresh_compiled_mut(&mut self) -> &mut bool {
        &mut self.is_fresh_compiled
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
    instr_list: BTreeSet<Instr<bool>>,
    compile_cache_ends: Vec<usize>,
    compile_cache_fns: Vec<Box<dyn FnTraitSet<bool>>>,
    is_fresh_compiled: bool,
}

impl DOChan {
    pub fn new(port: usize, line: usize, samp_rate: f64, dflt_val: bool, rst_val: bool) -> Self {
        Self {
            port,
            line,
            samp_rate,
            dflt_val,
            rst_val,
            instr_list: BTreeSet::new(),
            compile_cache_ends: Vec::new(),
            compile_cache_fns: Vec::new(),
            is_fresh_compiled: true,
        }
    }

    pub fn port(&self) -> usize {
        self.port
    }

    pub fn line(&self) -> usize {
        self.line
    }

    /// Generates full channel name string based on port and line indices - the single place where
    /// the naming format of DO channels is written out explicitly.
    ///
    /// Use this function wherever channel name string is required instead of assembling it
    /// manually with `format!(...)`. This will ensure everything automatically stays consistent
    /// throughout the project even if naming format has to be changed.
    pub fn name_fmt(port: usize, line: usize) -> String {
        format!("port{port}/line{line}")
    }
}

impl BaseChan for DOChan {
    type Samp = bool;

    fn name(&self) -> String {
        DOChan::name_fmt(self.port, self.line)
    }

    fn samp_rate(&self) -> f64 {
        self.samp_rate
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

    fn is_fresh_compiled(&self) -> bool {
        self.is_fresh_compiled
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

    fn is_fresh_compiled_mut(&mut self) -> &mut bool {
        &mut self.is_fresh_compiled
    }
}

// region DO Port channel
pub struct DOPort {
    pub idx: usize,
    pub ends: Vec<usize>,
    pub vals: Vec<u32>,
}

impl DOPort {
    pub fn total_samps(&self) -> usize {
        match self.ends.last() {
            Some(&instr_end) => instr_end,
            None => 0,
        }
    }

    pub fn fill_samps(&self, window_start: usize, samp_buf: &mut [u32]) -> Result<(), String> {
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