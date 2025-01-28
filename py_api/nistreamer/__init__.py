from .streamer import NIStreamer
from nistreamer_backend import StdFnLib as _StdFnLib
std_fn_lib = _StdFnLib()

try:
    from nistreamer_backend import UsrFnLib as _UsrFnLib
    usr_fn_lib = _UsrFnLib()
except ImportError:
    # Backend was compiled without UsrFnLib feature
    pass
