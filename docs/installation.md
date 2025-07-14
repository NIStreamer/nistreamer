# Installation instructions
Currently, you have to git-clone the repos and build the back-end locally. The step-by-step guide is provided below. A pre-built and `pip`-installable version is coming later. 

Ensure you have the official [NI-DAQmx driver](https://www.ni.com/en/support/downloads/drivers/download.ni-daq-mx.html) installed before starting – `nistreamer` is only a layer on top of the driver and will not work without it.


## Get the tools
Install Rust by following the [official instructions](https://www.rust-lang.org/learn/get-started). This process will install all necessary Rust components including `cargo` – the command-line tool we will be using to compile the backend. You can verify that `cargo` was installed by running `cargo --version`.

We also need [maturin](https://www.maturin.rs/) to install compiled backend as a Python package. `maturin` itself is shipped as a Python package, so activate the desired environment (e.g. `conda activate ...`) where you want `nistreamer` to be eventually installed and run `pip install maturin`.

Finally, we recommend installing `git` – the command-line version control tool. This is the best way to get `nistreamer` source code you need and also pull the updates in the future. Download and run the [official installer](https://git-scm.com/downloads).


## Basic installation

Once you have all the tools installed, you can build `nistreamer`:

1. Get the full source code. It is hosted on [GitHub](https://github.com/pulse-streamer) and is split into two different repositories - `base-streamer` and `ni-streamer`. The best way is to clone them with `git`:
   ```
   git clone https://github.com/pulse-streamer/base-streamer.git
   git clone https://github.com/pulse-streamer/ni-streamer.git
   ```

2. Now compile the backend. In terminal, navigate to `/ni-streamer/backend` directory (this location should have `Cargo.toml` file) and run:
   ```
   cargo build --release
   ```
   If there are any compile errors, try checking [troubleshooting guide](#troubleshooting). Next run:
   ```
   maturin develop --release
   ```  
   to install the compiled backend as `nistreamer_backend` package into your Python environment. Don't forget to activate your desired target environment (e.g. `conda activate ...`) before running this command.


3. Now the front-end. It is a pure Python package and does not need to be installed. You only need to add its' location to Python's search path. The safest way is to do it locally in each of your scripts:
   ```Python
   import sys
   import os
   sys.path.append(os.path.join(r'/absolute/path/ni-streamer/py_api'))
   ```  
   Alternatively, you can append the path directly to the system-wide `PYTHONPATH` variable.  


All set! You should now be able to import and use the streamer:  
```Python
from nistreamer import NIStreamer, std_fn_lib 
from nistreamer import usr_fn_lib  # only if this feature was enabled, see below
```  
See the [demos](https://github.com/pulse-streamer/ni-streamer/tree/main/py_api/demo) for a quick-start guide.


## Advanced build options
Backend can be compiled with optional build features:
```
cargo build --release --features feature_name
maturin develop --release --features feature_name
```
The following features are available:

| Name            | Desctiption                                                                                                                       |
|-----------------|-----------------------------------------------------------------------------------------------------------------------------------|
| `usr_fn_lib`    | Include user function library. `usr-fn-lib` crate should be placed in the same directory with `base-streamer` and `ni-streamer`   |
| `nidaqmx_dummy` | Compile with a "dummy" instead of the actual NI-DAQmx driver. Helpful for development on a computer without the driver installed. |

Multiple features can be combined: `--features "usr_fn_lib, nidaqmx_dummy"`


## Troubleshooting

### cargo is not recognized as an internal or external command
If you installed Rust and still get the error indicating that `cargo` is not recognized, ensure that Rust binaries are added to your system's PATH. The installer typically does this, but in case it didn't, add `C:\Users\<YOUR_USERNAME>\.cargo\bin` to your system's PATH.

### Cannot open input file `NIDAQmx.lib`
Error message ending with the following line:
```
note: LINK : fatal error LNK1181: cannot open input file 'NIDAQmx.lib'
```
means that Cargo did not find the NIDAQmx static library which has to be present on the computer to compile the backend (unless you are compiling with `nidaqmx_dummy` feature, see [advanced build options](#advanced-build-options)). On Windows, the following location of NIDAQmx library is assumed by default:
```
C:/Program Files (x86)/National Instruments/Shared/ExternalCompilerSupport/C/lib64/msvc/NIDAQmx.lib
```
If on your system it is located somewhere else, update the linker arguments in `ni-streamer/backend/.cargo/config.toml` with the correct full path.

