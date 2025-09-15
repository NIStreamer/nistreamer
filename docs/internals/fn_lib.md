# Extensible Waveform Library
To generate the signal, streamer computes output values for a grid of sample clock time points. 
For each instruction, values are computed by evaluating the contained mathematical function 
(polynomial, sine, exponent, and so on) with specified parameters. 
We interchangeably call them "waveform functions", "waveforms", or simply "functions".

The main challenge is that waveform library has to be written in Rust 
(sample computation must be fast), but at the same time it needs to be _extensible_. 

As project is evolving, more functions will be added. Moreover, users may need to introduce custom 
functions for their specific applications. There should be a clean way of adding new waveforms 
without making any changes to the codebase core.

This section describes the implemented library mechanism.

## Waveforms as trait objects
**Problem 1** - _treating diverse waveforms in a uniform way_.

We represent each waveform by a struct containing corresponding function parameters like this:
```Rust
/// Linear function:
/// `LinFn(t) = slope*t + offs`
pub struct LinFn {
    slope: f64,
    offs: f64
}
```
So technically, different waveforms have completely different types. But we need a way to threat them uniformly - 
to store them in the same collection (edit cache and compile cache), and to process them during compilation and streaming.

A simple approach is to [use enums](https://doc.rust-lang.org/book/ch08-01-vectors.html#using-an-enum-to-store-multiple-types).
However, every new waveform would require adding a new enum variant and, consequently, making explicit edits 
throughout the back-end core to add a corresponding match arm.

A more natural and scalable approach is treating waveforms as [trait objects](https://doc.rust-lang.org/book/ch18-02-trait-objects.html)
based on the common behavior we rely on - computing signal value array for a given time array.
We formalize this behavior as the following trait:
```Rust
pub trait Calc<T> {
    fn calc(&self, t_arr: &[f64], res_arr: &mut [T]);
}
```
where `T` is the output sample type (`f64` for AO, `bool` for DO). 
Each waveform must implement this trait, essentially providing the mathematical expression for computing it:
```Rust
impl Calc<f64> for LinFn {
    fn calc(&self, t_arr: &[f64], res_arr: &mut[f64]) {
        for (res, &t) in res_arr.iter_mut().zip(t_arr.iter()) {
            *res = self.slope * t + self.offs
        }
    }
}
```
Practically, more assumptions are needed, so the actual trait in use is called 
`FnTraitSet<T>` and is derived from `Calc<T>` by requiring additional technical traits 
(see `nistreamer-base/src/fn_lib_base.rs` for details). 

That way all functions are treated uniformly as trait objects:
```Rust
Box<dyn FnTraitSet<T>>
```
and expanding function library does not require any additional changes anywhere.

## Waveform library classes
**Problem 2** - _avoiding explicit relay code at the Rust-Python interface_.

Waveforms are defined in Rust back-end, while user script makes selections through Python front-end -
there has to be a relay mechanism between the two. And it has to allow for extensibility as well - 
no manual edits should be required as new waveforms are added to the library. 
Implemented mechanism is described below. 

Waveforms are bundled into two libraries - the built-in `std_fn_lib` ("standard function library")
and an optional user-editable `usr_fn_lib`. Both are implemented in the same way, we focus on
`std_fn_lib` in this section, details of `usr_fn_lib` are discussed later.

Each library is defined as an empty Rust struct and is exposed as a Python class 
separately from the main `StreamerWrap` (see {ref}`this schematic <rust-py-interface>`):
```Rust
#[pyclass]
pub struct StdFnLib {}
```

For each waveform, `StdFnLib` implements and exposes a dedicated Python method named after this waveform:
```Rust
#[pymethods]
impl StdFnLib {
    fn LinFn(&self, slope: f64, offs: f64) -> PyResult<FnBoxF64> {
        let fn_inst = LinFn::new(slope, offs);
        let fn_box = FnBoxF64 { inner: Box::new(fn_inst) };
        Ok(fn_box)
    }
}
```
This method first creates a `LinFn` instance and then returns it as `Box<dyn FnTraitSet<f64>>` _into Python_.
Here the returned type `FnBoxF64` is a thin wrapper:
```Rust
#[pyclass]
pub struct FnBoxF64 {
    pub inner: Box<dyn FnTraitSet<f64>>
}
```
which is necessary because `#[pyclass]` cannot be attached directly to `Box<dyn FnTraitSet<f64>>` since PyO3 macros
don't support type parameters. 

The waveform instance is then passed back into Rust to form a new instruction on a specific channel:
```Rust
#[pymethods]
impl AOChan {
    pub fn add_instr(&mut self, func: FnBoxF64, t: f64, dur: f64)
}
```
(simplified for clarity, the actual methods are `StreamerWrap::ao/do_chan_add_instr` in `src/flat_wrap.rs`).
A schematic of this process is shown in {ref}`this figure <rust-py-interface>`.

For convenience, waveform library classes are instantiated in `py_api/nistreamer/__init__.py`:
```Python
from ._nistreamer import StdFnLib as _StdFnLib
std_fn_lib = _StdFnLib()
```

From the user script side, the whole process of creating and adding a waveform looks like this:
```Python
from nistreamer import std_fn_lib

# ... streamer setup ...

ao_chan.add_instr(
    func=std_fn_lib.LinFn(slope=1.0, offs=2.0),  # <-- waveform instance is passed across Python here
    t=0.0, dur=1e-3
)
```

Exposing waveform libraries as separate Python classes allows for the following key properties:
* Streamer core only exposes a single entry point for all waveforms - the `add_instr` methods of  `StreamerWrap`;
* Waveform library `pyclass`es are exposed publicly (in contrast to `StreamerWrap`, which is hidden behind PyAPI),
  allowing direct use of the named methods `PyO3` macros automatically generated for every waveform .

As a result, there is no explicit relay code, so adding new waveforms to libraries does not require making 
any edits in the back-end core or PyAPI, as required for extensibility.

## User-editable library
**Problem 3** - _a place for custom waveforms_.

It is always possible that a user needs a highly-specialized waveform which is not available 
in the built-in library. The user-editable `usr_fn_lib` is introduced for such cases 
to allow for adding custom waveforms locally.

This library is contained in a separate crate `nistreamer-usrlib` and is marked as an optional dependency
behind `usrlib` feature:
- by default, back-end compiles without it;
- while compiling with `--features usrlib` flag includes it.

(see {doc}`compilation instructions </getting_started/installation>` for details).

Moreover, `nistreamer-usrlib` is split out into a separate `git`/`GitHub` [repository](https://github.com/NIStreamer/nistreamer-usrlib)
to maximally decouple it from the rest of the codebase and simplify version control task for the users. 
Users are advised to create a fork of this GitHub repository to simplify bringing in any future updates.

If compiled with `usrlib` feature, user library can be imported and used alongside with `std_fn_lib`: 
```Python
from nistreamer import std_fn_lib, usr_fn_lib

# ... streamer setup ...

ao_chan.add_instr(
    func=std_fn_lib.LinFn(slope=1.0, offs=2.0),
    t=0.0, dur=1e-3
)

ao_chan.add_instr(
    func=usr_fn_lib.GaussSine(t0=3e-3, sigma=0.5e-3, scale=1.5, freq=10e3),
    t=2e-3, dur=2e-3
)
```

### Helper procedural macros
Apart from being located in a separate crate and a separate repository, `usr_fn_lib` is implemented
in the same way as `std_fn_lib`. The example below shows what a minimal version of `nistreamer-usrlib/src/lib.rs`
would look like with just a single custom waveform `MyLinFn`:
```Rust
// Uses:
use pyo3::prelude::*;
use nistreamer_base::fn_lib_base::{Calc, FnBoxF64, FnBoxBool};

// `UsrFnLib` setup:
#[pyclass]
pub struct UsrFnLib {}
#[pymethods]
impl UsrFnLib {
    #[new]
    pub fn new() -> Self {
        Self {}
    }
}

// Custom waveform:
#[derive(Clone, Debug)]
pub struct MyLinFn {
    slope: f64,
    offs: f64,
}
impl MyLinFn {
    pub fn new(slope: f64, offs: f64) -> Self {
        Self { slope, offs }
    }
}
#[pymethods]
impl UsrFnLib {
    /// Linear function:
    ///     `MyLinFn(t) = slope*t + offs`
    #[allow(non_snake_case)]
    fn MyLinFn(&self, slope: f64, offs: f64) -> PyResult<FnBoxF64> {
        let fn_inst = MyLinFn::new(slope, offs);
        let fn_box = FnBoxF64 { inner: Box::new(fn_inst) };
        Ok(fn_box)
    }
}
impl Calc<f64> for MyLinFn { /* implementation here */ }
```
Then PyO3 macros generate `UsrFnLib` Python class with a method `MyLinFn`,
with the doc-comment automatically relayed into Python docstring:
```
Signature: usr_fn_lib.MyLinFn(slope, offs)
Docstring:
Linear function:
    `MyLinFn(t) = slope*t + offs`
Type:      builtin_function_or_method
```

This version would be completely acceptable for the built-in `std_fn_lib`, 
but is problematic for `usr_fn_lib`:

- There are many internal details, that are not meant to be a part of public API but still have to be 
  written explicitly (e.g. type names `UsrFnLib`, `FnBoxF64`, location of `fn_lib_base`, and so on). 
  These details may change in the future, resulting in unnecessary breaking changes.

- Users have to manually write the `#[pymethods] impl UsrFnLib { ... }` block for each new waveform, 
  which may be tedious and intimidating (we don't assume Rust proficiency).

To address these issues, we hide most of the code into procedural macros. 
Macros reduce the above example to the following:
```Rust
use nistreamer_base::usrlib_prelude::*;
usrlib_boilerplate!();

/// Linear function:
///     `MyLinFn(t) = slope*t + offs`
#[usr_fn_f64]
pub struct MyLinFn {
    slope: f64,
    offs: f64,
}
impl Calc<f64> for MyLinFn { /* implementation here */ }
```
Here:
- `usrlib_prelude` module bundles all necessary imports;
- `usrlib_boilerplate!()` function-like macro expands to the `UsrFnLib` setup;
- `usr_fn_f64` attribute-like macro automatically generates the `impl MyLinFn {pub fn new(...)}` 
  and `#[pymethods] impl UsrFnLib { fn MyLinFn(...) }` blocks based on the struct contents 
  it is attached to.

So for each new waveform, users only have to write the definition of the struct
and `Calc<T>` trait implementation. Then it is sufficient to attach one of the two macros - `usr_fn_f64` 
for analog or `usr_fn_bool` for digital waveforms - which auto-generates the rest of the code.

We also utilize the optional `#[pyo3(signature = (...))]` attribute to specify default parameter 
values for the generated Python methods by passing it the token stream from `usr_fn_f64` argument:
```Rust
/// Linear function:
///     `MyLinFn(t) = slope*t + offs`
/// `offs` is optional and defaults to 0.0
#[usr_fn_f64(slope, offs=0.0)]
pub struct MyLinFn {
    slope: f64,
    offs: f64,
}
```
results in `MyLinFn(slope, offs=0.0)` Python method.

Although this is not strictly necessary, there are two similar macros - `std_fn_f64` and `std_fn_bool` -
which are used for most waveforms in `std_fn_lib` and are based on the same logic as `usr_fn_f64`.
All macros are defined in a separate sub-package `nistreamer-macros`.

As a technical note, we rely on `multiple-pymethods` feature of `PyO3` to collect methods from all 
`#[pymethods] impl UsrFnLib` blocks together.
