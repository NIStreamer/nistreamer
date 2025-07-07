# `ni-streamer`

This repository is a part of `PulseStreamer` project and contains concrete implementations for National Instruments DAQ hardware. See [main page](https://github.com/pulse-streamer) for project details. 

## Backend
`/backend` directory contains `nistreamer_backend` Rust crate:
   * Rust wrapper of `NI-DAQmx` C API;
   * Structs for AO and DO channels and devices;
   * `Streamer` struct (contains thread management logic);
   * `flat_wrap.rs` contains `StreamerWrap` struct which exposes all channel/device/streamer methods in a flat way for `PyO3` wrapping.

Building this crate with `maturin --develop` installs `nistreamer_backend` package in the current Python environment. The package will contain `StreamerWrap` and `StdFnLib` classes. If `--features usr_fn_lib` is used, there will also be `UsrFnLib` class.  


## PyAPI
`/py_api` directory contains `nistreamer` package - this is the user-facing Python API. 

It relies on the installed `nistreamer_backend` and its main function is to restore the original object-oriented syntax of channel/device access which had to be "flattened" into a single `StreamerWrap` when going through Rust-Python interface. It is done by representing individual channels and devices with "proxies". All proxies have direct access to the same underlying `StreamerWrap` instance, but each exposes the relevant methods only and automatically passes the corresponding channel/device name arguments when relaying the call to `StreamerWrap`.
