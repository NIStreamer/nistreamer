"""Channel module.

Contains the ``BaseChanProxy`` and ``AO/DOChanProxy`` classes
representing individual channels.
"""

from ._nistreamer import StreamerWrap as _StreamerWrap
from ._nistreamer import StdFnLib
from abc import ABC, abstractmethod
from typing import Optional, Union
import math


class BaseChanProxy(ABC):
    """The base of channel proxy classes."""

    def __init__(self,
                 _streamer: _StreamerWrap,
                 _card_max_name: str,
                 nickname: str = None):

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
    def chan_name(self) -> str:
        """Physical channel name."""
        # Channel naming format is set in Rust backed.
        # One should call StreamerWrap method to get the name string instead of assembling it manually here.
        # Since StreamerWrap's methods for AO and DO cards are different,
        # each subclass has to re-implement this property to call the corresponding Rust method
        pass

    @property
    def nickname(self) -> str:
        """Human-readable channel name used in visualizations.

        Nickname is set when channel is added to the card.
        If no nickname was specified, :meth:`chan_name` is used instead.
        """
        if self._nickname is not None:
            return self._nickname
        else:
            return self.chan_name

    @property
    @abstractmethod
    def dflt_val(self):
        """The default value for intervals that are not covered by instructions."""
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this property
        pass

    @property
    @abstractmethod
    def rst_val(self):
        """The value set by :meth:`~nistreamer.streamer.NIStreamer.add_reset_instr` command."""
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this property
        pass

    @abstractmethod
    def _add_instr(self, func, t, dur_spec):
        """The base of add-instruction methods containing the actual call to ``StreamerWrap``.

        Args:
            func: waveform function instance
            t (float): start time
            dur_spec: duration specification. Either ``tuple(dur: float, keep_val: bool)``
                for finite or ``None`` for unspecified duration.
        """
        # AO and DO cards accept different function object types and thus call different Rust functions.
        # so each subclass has to re-implement this method
        pass

    def add_instr(self, func, t: float, dur: float, keep_val: Optional[bool] = False) -> float:
        """Add a finite-duration instruction.

        Args:
            func: waveform function instance
            t: start time
            dur: duration
            keep_val: determines the constant value after instruction end. If ``True``,
                the last waveform value is kept, otherwise channel goes to default.

        Returns:
            Instruction duration

        Raises:
            ValueError: if this instruction collides with an existing one
            TypeError: if waveform function does not mach channel type (analog/digital)

        Examples:
            >>> from nistreamer import NIStreamer, std_fn_lib
            >>>
            >>> strmr = NIStreamer()
            >>> ao_card = strmr.add_ao_card(max_name='Dev1', samp_rate=1e6)
            >>> ao_0 = ao_card.add_chan(chan_idx=0)
            >>>
            >>> ao_0.add_instr(
            >>>     func=std_fn_lib.LinFn(slope=1, offs=2),
            >>>     t=1.0,
            >>>     dur=2.0,
            >>>     keep_val=True
            >>> )
        """
        self._add_instr(func=func, t=t, dur_spec=(dur, keep_val))
        return dur

    def add_gothis_instr(self, func, t):
        """Add an instruction with unspecified duration.

        A so-called "go-this" instruction. During compilation, it will automatically
        fill the full interval until the next instruction start or global sequence end.

        Args:
            func: waveform function instance
            t: start time

        Returns:
            Instruction duration

        Raises:
            ValueError: if this instruction collides with an existing one
            TypeError: if waveform function does not mach channel type (analog/digital)

        Examples:
            >>> from nistreamer import NIStreamer, std_fn_lib
            >>>
            >>> strmr = NIStreamer()
            >>> ao_card = strmr.add_ao_card(max_name='Dev1', samp_rate=1e6)
            >>> ao_0 = ao_card.add_chan(chan_idx=0)
            >>>
            >>> ao_0.add_gothis_instr(
            >>>     func=std_fn_lib.Sine(amp=1.0, freq=2e3),
            >>>     t=1.0,
            >>> )
            >>> strmr.compile(stop_time=10.0)
        """
        self._add_instr(func=func, t=t, dur_spec=None)

    def last_instr_end_time(self) -> Union[float, None]:
        """Returns the last instruction end time or ``None`` if the edit cache is empty."""
        return self._streamer.chan_last_instr_end_time(
            dev_name=self._card_max_name,
            chan_name=self.chan_name
        )

    def clear_edit_cache(self):
        """Discards all instructions from this channel."""
        self._streamer.chan_clear_edit_cache(
            dev_name=self._card_max_name,
            chan_name=self.chan_name
        )

    @abstractmethod
    def calc_signal(self,
                    start_time: Union[float, None] = None,
                    end_time: Union[float, None] = None,
                    nsamps: Optional[int] = 1000):
        """Computes channel values for an array of time points.

        This function is exposed for channel signal plotting. The ``nsamps`` time points
        are distributed uniformly over the closed interval ``[start_time, end_time]``.

        Args:
            start_time: interval start. If ``None``, zero time is used
            end_time: interval end. If ``None``, sequence end is used
            nsamps: number of points

        Returns:
            List of corresponding channel values

        Raises:
            ValueError: if any parameters are invalid, if sequence is not fresh-compiled.

        Notes:
            You may need to select a sufficiently large ``nsamps`` and a sufficiently
            narrow time window to see the true waveform shape that the actual stream
            would produce when sampling at the hardware clock rate. Otherwise, very
            narrow pulses may be missed and periodic waveforms may appear distorted
            due to undersampling.
        """
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this method
        pass

    @abstractmethod
    def eval_point(self, t: float):
        """Computes channel value at time point ``t``.

        Unlike :meth:`calc_signal`, this method does not require sequence compilation.
        So it can be used in sequence scripting, for instance to "measure" start
        values for ramps.

        **CAUTION!** The returned value is computed according to the *current*
        edit cache, and does not update with any future sequence edits.
        If later in the script, an instruction is added at earlier time
        changing the actual value at point ``t``, there may be a discontinuity
        since the ramp will use the outdated start value that was given to it.
        """
        # AO and DO cards have different sample types and thus call different Rust functions,
        # so each subclass has to re-implement this method
        pass


class AOChanProxy(BaseChanProxy):
    """Analog output channel proxy."""
    def __init__(self,
                 _streamer: _StreamerWrap,
                 _card_max_name: str,
                 chan_idx: int,
                 nickname: str = None):

        BaseChanProxy.__init__(
            self,
            _streamer=_streamer,
            _card_max_name=_card_max_name,
            nickname=nickname
        )
        self.chan_idx = chan_idx

    @property
    def chan_name(self) -> str:
        return self._streamer.ao_chan_name(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx
        )

    @property
    def dflt_val(self) -> float:
        return self._streamer.ao_chan_dflt_val(
            dev_name=self._card_max_name,
            chan_idx=self.chan_idx
        )

    @property
    def rst_val(self) -> float:
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

    def eval_point(self, t) -> float:
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
    def const(self, t: float, dur: float, val: float) -> float:
        """Constant-value pulse with a fixed duration.

        Args:
            t: start time
            dur: pulse duration
            val: value

        Returns:
            Pulse duration

        Raises:
            ValueError: if this instruction collides with an existing one

        Notes:
            This method does not have the "keep value" option - the channel will
            transition to the default value at the end of this pulse. Use
            :meth:`go_const` if you want to set and keep the constant value instead.
        """
        return self.add_instr(
            func=self._std_fn_lib.ConstF64(val=val),
            t=t,
            dur=dur,
            keep_val=False
        )
    
    def go_const(self, t: float, val: float):
        """Set a constant value ``val`` at time ``t`` and keep it until further instructions.

        During compilation, this instruction will automatically fill the full
        interval until the next instruction start or global sequence end.
        """
        self.add_gothis_instr(
            func=self._std_fn_lib.ConstF64(val=val),
            t=t
        )

    def sine(self,
             t: float,
             dur: float,
             amp: float,
             freq: float,
             phase: Optional[float] = 0,
             offs: Optional[float] = 0,
             keep_val: Optional[bool] = False) -> float:
        """Sinusoidal pulse with a fixed duration.

        The waveform is parametrized as follows:
        ``Sine(t) = amp * sin(2Pi * freq * t + phase) + offs``

        Args:
            t: start time
            dur: pulse duration
            amp: amplitude (Volts)
            freq: linear frequency (Hz, 1/period)
            phase: absolute phase (radians)
            offs: constant offset (Volts)
            keep_val: if ``True``, the last value will be kept after the pulse,
                otherwise channel goes to default value.

        Returns:
            Pulse duration ``dur``

        Raises:
            ValueError: if this instruction collides with an existing one
        """
        return self.add_instr(
            func=self._std_fn_lib.Sine(amp=amp, freq=freq, phase=phase, offs=offs),
            t=t,
            dur=dur,
            keep_val=keep_val
        )
    
    def go_sine(self,
                t: float,
                amp: float,
                freq: float,
                phase: Optional[float] = 0,
                offs: Optional[float] = 0):
        """Sinusoidal pulse without a specified duration.

        During compilation, this instruction will automatically fill the full
        interval until the next instruction start or global sequence end.

        The waveform is parametrized as follows:
        ``Sine(t) = amp * sin(2Pi * freq * t + phase) + offs``,

        Args:
            t: start time
            amp: amplitude (Volts)
            freq: linear frequency (Hz, 1/period)
            phase: absolute phase (radians)
            offs: constant offset (Volts)

        Raises:
            ValueError: if this instruction collides with an existing one
        """
        self.add_gothis_instr(
            func=self._std_fn_lib.Sine(amp=amp, freq=freq, phase=phase, offs=offs),
            t=t
        )

    def linramp(self,
                t: float,
                dur: float,
                start_val: float,
                end_val: float,
                keep_val: Optional[bool] = True) -> float:
        """Linear ramp.

        Connects the points ``(t, start_val)`` and ``(t + dur, end_val)``
        with a linear function. If ``keep_val=True``, the end value will be kept
        after the pulse, otherwise channel goes to default value.

        Returns:
            Duration ``dur``

        Raises:
            ValueError: if this instruction collides with an existing one
        """
        # Calculate linear function parameters y = slope*x + offs
        slope = (end_val - start_val) / dur
        offs = ((t + dur) * start_val - t * end_val) / dur

        return self.add_instr(
            func=self._std_fn_lib.LinFn(slope=slope, offs=offs),
            t=t,
            dur=dur,
            keep_val=keep_val
        )

    def sineramp(self,
                 t: float,
                 dur: float,
                 start_val: float,
                 end_val: float,
                 keep_val: Optional[bool] = True) -> float:
        """Sinusoidal ramp.

        Connects the points ``(t, start_val)`` and ``(t + dur, end_val)`` with
        a half-period of a sine function such that the derivative is zero on both ends.
        If ``keep_val=True``, the end value will be kept after the pulse,
        otherwise channel goes to default value.

        Returns:
            Duration ``dur``

        Raises:
            ValueError: if this instruction collides with an existing one
        """
        amp = (end_val - start_val) / 2
        offs = (end_val + start_val) / 2
        period = 2 * dur
        freq = 1 / period
        phase = -2*math.pi * (t/period + 1/4)

        return self.add_instr(
            func=self._std_fn_lib.Sine(amp=amp, freq=freq, phase=phase, offs=offs),
            t=t,
            dur=dur,
            keep_val=keep_val
        )
    # endregion


class DOChanProxy(BaseChanProxy):
    """Digital output channel proxy (an individual digital line)."""
    def __init__(self,
                 _streamer: _StreamerWrap,
                 _card_max_name: str,
                 port_idx: int,
                 line_idx: int,
                 nickname: str = None):

        BaseChanProxy.__init__(
            self,
            _streamer=_streamer,
            _card_max_name=_card_max_name,
            nickname=nickname
        )
        self.port = port_idx
        self.line = line_idx

    @property
    def chan_name(self) -> str:
        return self._streamer.do_chan_name(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def dflt_val(self) -> bool:
        return self._streamer.do_chan_dflt_val(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def rst_val(self) -> bool:
        return self._streamer.do_chan_rst_val(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line
        )

    @property
    def const_fns_only(self) -> bool:
        """Shows if the host card has the "constant functions only" mode enabled.

        If enabled, all lines on this card will only accept the following four
        constant-valued instructions: :meth:`high`, :meth:`low`, :meth:`go_high`,
        and :meth:`go_low`.
        This restriction allows to accelerate the runtime sample computation
        and significantly reduce the risk of buffer underflow.

        In most cases, only constant-valued instructions are used anyway,
        so this mode is enabled by default. If you need to add non-constant
        boolean waveforms, set :meth:`~nistreamer.card.DOCardProxy.const_fns_only`
        of the host card to ``False``.
        """
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

    def eval_point(self, t) -> bool:
        return self._streamer.do_chan_eval_point(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line,
            t=t
        )

    def _unchecked_add_instr(self, func, t, dur_spec):
        # The actual call to `StreamerWrap.add_instr`.
        # This function is separate from `_add_instr` to implement `const_fns_only` mode checking.
        self._streamer.do_chan_add_instr(
            dev_name=self._card_max_name,
            port=self.port,
            line=self.line,
            func=func,
            t=t,
            dur_spec=dur_spec
        )

    def _add_instr(self, func, t, dur_spec):
        # This function use is rejected whenever `const_fns_only` is `True`
        # to prevent addition of non-constant-valued functions
        if self.const_fns_only:
            raise ValueError(
                "Constant-functions-only mode is currently enabled for this device\n"
                "* If you wanted to add a simple high/low/go_high/go_low instruction, "
                "use the corresponding named method instead.\n"
                "* If you actually wanted to add a generic non-constant function, "
                "you have to set `your_do_card.const_fns_only = False` to disable this mode.\n"
                "See docs for details about const-fns-only mode and performance considerations."
            )
        self._unchecked_add_instr(func=func, t=t, dur_spec=dur_spec)

    # region Convenience methods to access the most common StdFnLib functions
    def go_high(self, t: float):
        """Sets the logical high at time ``t`` and keeps it until the next instruction start / sequence end."""
        # This is one of the four possible constant-valued boolean instructions and can be added
        # even if `const_fns_only = True`, so using `unchecked_add` directly:
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=True),
            t=t,
            dur_spec=None
        )

    def go_low(self, t: float):
        """Sets the logical low at time ``t`` and keeps it until the next instruction start / sequence end."""
        # This is one of the four possible constant-valued boolean instructions and can be added
        # even if `const_fns_only = True`, so using `unchecked_add` directly:
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=False),
            t=t,
            dur_spec=None
        )

    def high(self, t: float, dur: float) -> float:
        """Logical-high pulse from time ``t`` to ``t + dur``.

        Returns:
            Pulse duration ``dur``

        Raises:
            ValueError: if this instruction collides with an existing one

        Notes:
            This method does not have the "keep value" option - the channel will
            transition to the default value at the end of this pulse. Use :meth:`go_high`
            instruction if you want to set and keep the constant value instead.
        """
        # This is one of the four possible constant-valued boolean instructions and can be added
        # even if `const_fns_only = True`, so using `unchecked_add` directly:
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=True),
            t=t,
            dur_spec=(dur, False)
        )
        return dur

    def low(self, t: float, dur: float) -> float:
        """Logical-low pulse from time ``t`` to ``t + dur``.

        Returns:
            Pulse duration ``dur``

        Raises:
            ValueError: if this instruction collides with an existing one

        Notes:
            This method does not have the "keep value" option - the channel will
            transition to the default value at the end of this pulse. Use :meth:`go_low`
            instruction if you want to set and keep the constant value instead.
        """
        # This is one of the four possible constant-valued boolean instructions and can be added
        # even if `const_fns_only = True`, so using `unchecked_add` directly:
        self._unchecked_add_instr(
            func=self._std_fn_lib.ConstBool(val=False),
            t=t,
            dur_spec=(dur, False)
        )
        return dur
    # endregion
