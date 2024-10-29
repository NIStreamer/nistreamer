from .streamer import NIStreamer
from nistreamer_backend import StdFnLib as _StdFnLib
from nistreamer_backend import UsrFnLib as _UsrFnLib

std_fn_lib = _StdFnLib()
usr_fn_lib = _UsrFnLib()
