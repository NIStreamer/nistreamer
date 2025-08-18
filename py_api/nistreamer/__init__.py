"""NIStreamer - an abstraction layer for scripted pulse sequence generation with National Instruments hardware.

This package is the Python front-end
"""

from .streamer import NIStreamer
from ._nistreamer import StdFnLib as _StdFnLib
std_fn_lib = _StdFnLib()

try:
    from ._nistreamer import UsrFnLib as _UsrFnLib
    usr_fn_lib = _UsrFnLib()
except ImportError:
    # Backend was compiled without UsrFnLib feature
    pass
