# `ni-streamer`

This repository is the head of NI Pulse Streamer project. It contains the following parts:
   * The head crate of Rust back-end;
   * Python front-end; 
   * Full project documentation. 

See [main documentation page](https://github.com/pulse-streamer) for project details. See the main GitHub organization for. 

## Back-end
 (concrete implementations for National Instruments DAQ hardware)
`src` directory contains `_nistreamer` Rust crate:
   * Rust wrapper of `NI-DAQmx` C API;
   * Structs for AO and DO channels and devices;
   * `Streamer` struct (contains thread management logic);
   * `flat_wrap.rs` contains `StreamerWrap` struct which exposes all channel/device/streamer methods in a flat way for `PyO3` wrapping.

Once compiled, `_nistreamer.pyd` package will contain `StreamerWrap` and `StdFnLib` classes for use by the front-end `PyAPI`. If `--features usr_fn_lib` is used, there will also be `UsrFnLib` class.  

## PyAPI
`/py_api` directory contains `nistreamer` package - this is the user-facing Python API. 

It relies on the installed `nistreamer_backend` and its main function is to restore the original object-oriented syntax of channel/device access which had to be "flattened" into a single `StreamerWrap` when going through Rust-Python interface. It is done by representing individual channels and devices with "proxies". All proxies have direct access to the same underlying `StreamerWrap` instance, but each exposes the relevant methods only and automatically passes the corresponding channel/device name arguments when relaying the call to `StreamerWrap`.


## Building
Building this crate with `maturin --develop` installs `nistreamer` package in the current Python environment. Building with `maturin build` creates a distribution-ready wheel.
