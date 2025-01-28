from nistreamer.channel import DOChanProxy


class InvertedDOChan(DOChanProxy):
    @property
    def dflt_val(self):
        return 'Off' if super().dflt_val else 'On'

    @property
    def rst_val(self):
        return 'Off' if super().rst_val else 'On'

    def on(self, t, dur):
        return super().low(t=t, dur=dur)

    def off(self, t, dur):
        return super().high(t=t, dur=dur)

    def go_on(self, t):
        return super().go_low(t=t)

    def go_off(self, t):
        return super().go_high(t=t)
