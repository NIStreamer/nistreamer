from nistreamer import NIStreamer as _NIStreamer  # `_` prevents re-import with wildcard `import *`
ni_strmr = _NIStreamer()

ao_card = ni_strmr.add_ao_card(max_name='Dev2', samp_rate=1e6)
do_card = ni_strmr.add_do_card(max_name='Dev3', samp_rate=10e6)
# ... a few more cards registered here ...

ao_chan = ao_card.add_chan(chan_idx=0)
do_chan = do_card.add_chan(port_idx=0, line_idx=0)
# ... many more channels registered here ...

# Sync:
TRIG_LINE = 'RTSI0'
ao_card.start_trig_out = TRIG_LINE
do_card.start_trig_in = TRIG_LINE
# ... sync settings for other cards ...
ni_strmr.starts_last = ao_card.max_name
