"""NIStreamer - an abstraction layer for scripted pulse sequence generation with National Instruments hardware.

This package is the Python front-end
"""

from .streamer import NIStreamer
from nistreamer_backend import StdFnLib as _StdFnLib
std_fn_lib = _StdFnLib()

try:
    from nistreamer_backend import UsrFnLib as _UsrFnLib
    usr_fn_lib = _UsrFnLib()
except ImportError:
    # Backend was compiled without UsrFnLib feature
    pass
