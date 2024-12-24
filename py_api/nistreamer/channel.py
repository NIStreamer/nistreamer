from nistreamer_backend import StreamerWrap, StdFnLib
from abc import ABC, abstractmethod


class BaseChanProxy(ABC):
    def __init__(
            self,
            _streamer: StreamerWrap,
            _card_max_name: str,
            nickname: str = None
    ):
        self._streamer = _streamer
        self._card_max_name = _card_max_name
        self._nickname = nickname
        self._std_fn_lib = StdFnLib()

    def __repr__(self, card_info=False):
        return (
            f'Channel {self.chan_name} on card {self._card_max_name}\n'
            f'Default value: {self.dflt_val}\n'
            f'  Reset value: {self.rst_val}'
        )

    @property
    @abstractmethod
    def chan_name(self):
        # Channel naming format is set in Rust backed.
        # One should call StreamerWrap method to get the name string instead of assembling it manually here.
        # Since StreamerWrap's methods for AO and DO cards are different,
        # each subclass has to re-implement this property to call the corresponding Rust method
        pass

    @property
    def nickname(self):
        if self._nickname is not None:
            return self._nickname
        else:
            return self.chan_name

    @property
    @abstractmethod
    def dflt_val(self):
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this property
        pass

    @property
    @abstractmethod
    def rst_val(self):
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this property
        pass

    @abstractmethod
    def _add_instr(self, func, t, dur_spec):
        # AO and DO cards accept different function object types and thus call different Rust functions.
        # so each subclass has to re-implement this method
        pass

    def add_instr(self, func, t, dur, keep_val=False):
        self._add_instr(func=func, t=t, dur_spec=(dur, keep_val))
        return dur

    def add_gothis_instr(self, func, t):
        self._add_instr(func=func, t=t, dur_spec=None)

    def last_instr_end_time(self):
        return self._streamer.chan_last_instr_end_time(
            dev_name=self._card_max_name,
            chan_name=self.chan_name
        )

    def clear_edit_cache(self):
        self._streamer.chan_clear_edit_cache(
            dev_name=self._card_max_name,
            chan_name=self.chan_name
        )

    @abstractmethod
    def calc_signal(self, start_time=None, end_time=None, nsamps=1000):
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this method
        pass

    @abstractmethod
    def eval_point(self, t):
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this method
        pass


class AOChanProxy(BaseChanProxy):
    def __init__(
            self,
            _streamer: StreamerWrap,
            _card_max_name: str,
            chan_idx: int,
            nickname: str = None
    ):
        # ToDo[Tutorial]: pass through all arguments to parent's __init__, maybe with *args, **kwargs,
        #  but such that argument completion hints are still coming through.

        BaseChanProxy.__init__(
            self,
            _streamer=_streamer,
            _card_max_name=_card_max_name,
            nickname=nickname
        )
        self.chan_idx = chan_idx

    @property
    def chan_name(self):
        return self._streamer.ao_chan_name(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx
        )

    @property
    def dflt_val(self):
        return self._streamer.ao_chan_dflt_val(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx
        )

    @property
    def rst_val(self):
        return self._streamer.ao_chan_rst_val(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx
        )

    def calc_signal(self, start_time=None, end_time=None, nsamps=1000):
        return self._streamer.ao_chan_calc_nsamps(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx,
            n_samps=nsamps,
            start_time=start_time,
            end_time=end_time
        )

    def eval_point(self, t):
        return self._streamer.ao_chan_eval_point(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx,
            t=t
        )

    def _add_instr(self, func, t, dur_spec):
        self._streamer.ao_chan_add_instr(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx,
            func=func,
            t=t,
            dur_spec=dur_spec
        )

    # region Convenience methods to access the most common StdFnLib functions
    def const(self, t, dur, val):
        return self.add_instr(
            func=self._std_fn_lib.ConstF64(val=val),
            t=t,
            dur=dur,
            keep_val=False
        )
    
    def go_const(self, t, val):
        self.add_gothis_instr(
            func=self._std_fn_lib.ConstF64(val=val),
            t=t
        )

    def sine(self, t, dur, amp, freq, phase=0, dc_offs=0, keep_val=False):
        return self.add_instr(
            func=self._std_fn_lib.Sine(amp=amp, freq=freq, phase=phase, offs=dc_offs),
            t=t,
            dur=dur,
            keep_val=keep_val
        )
    
    def go_sine(self, t, amp, freq, phase=0, dc_offs=0):
        self.add_gothis_instr(
            func=self._std_fn_lib.Sine(amp=amp, freq=freq, phase=phase, offs=dc_offs),
            t=t
        )

    def linramp(self, t, dur, start_val, end_val, keep_val=True):
        # Calculate linear function parameters y = a*x + b
        a = (end_val - start_val) / dur
        b = ((t + dur) * start_val - t * end_val) / dur

        return self.add_instr(
            func=self._std_fn_lib.LinFn(a=a, b=b),
            t=t,
            dur=dur,
            keep_val=keep_val
        )
    # endregion


class DOChanProxy(BaseChanProxy):
    def __init__(
            self,
            _streamer: StreamerWrap,
            _card_max_name: str,
            port_idx: int,
            line_idx: int,
            nickname: str = None
    ):
        BaseChanProxy.__init__(
            self,
            _streamer=_streamer,
            _card_max_name=_card_max_name,
            nickname=nickname
        )
        self.port = port_idx
        self.line = line_idx

    @property
    def chan_name(self):
        return self._streamer.do_chan_name(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def dflt_val(self):
        return self._streamer.do_chan_dflt_val(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def rst_val(self):
        return self._streamer.do_chan_rst_val(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def const_fns_only(self):
        return self._streamer.dodev_get_const_fns_only(name=self._card_max_name)

    def calc_signal(self, start_time=None, end_time=None, nsamps=1000):
        return self._streamer.do_chan_calc_nsamps(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line,
            n_samps=nsamps,
            start_time=start_time,
            end_time=end_time
        )

    def eval_point(self, t):
        return self._streamer.do_chan_eval_point(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line,
            t=t
        )

    def _unchecked_add_instr(self, func, t, dur_spec):
        self._streamer.do_chan_add_instr(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line,
            func=func,
            t=t,
            dur_spec=dur_spec
        )

    def _add_instr(self, func, t, dur_spec):
        if self.const_fns_only:
            raise ValueError(
                "Constant-functions-only mode is currently enabled for this device\n"
                "* If you wanted to add a simple high/low/go_high/go_low instruction, "
                "use the corresponding named method\n"
                "* If you actually wanted to add a generic non-constant function, "
                "you have to set `your_do_card.const_fns_only = False` to disable this mode\n"
                "See docs for details about const-fns-only mode and performance considerations"
            )
        self._unchecked_add_instr(func=func, t=t, dur_spec=dur_spec)

    # region Convenience methods to access the most common StdFnLib functions
    def go_high(self, t):
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=True),
            t=t,
            dur_spec=None
        )

    def go_low(self, t):
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=False),
            t=t,
            dur_spec=None
        )

    def high(self, t, dur):
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=True),
            t=t,
            dur_spec=(dur, False)
        )
        return dur

    def low(self, t, dur):
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=False),
            t=t,
            dur_spec=(dur, False)
        )
        return dur
    # endregion
