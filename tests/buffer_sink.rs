use drywet::sink::BufferSink;

#[test]
fn buffer_sink_mix_at_offset_and_write_cursor() {
    let mut sink = BufferSink::new(8, 1);
    sink.mix(&[1.0, 1.0], Some(2));
    assert_eq!(sink.frames(), &[0.0, 0.0, 1.0, 1.0]);
    sink.write(&[0.5, 0.5]);
    assert_eq!(sink.write_cursor(), 2);
    assert_eq!(&sink.frames()[0..2], &[0.5, 0.5]);
    assert!(sink.accepted());
}

#[test]
fn buffer_sink_tails_sum_and_stereo_duplicates() {
    let mut sink = BufferSink::new(4, 1);
    sink.mix(&[0.2, 0.2], Some(0));
    sink.mix(&[0.3], Some(1));
    assert_eq!(sink.frames(), &[0.2, 0.5]);

    let mut stereo = BufferSink::new(4, 2);
    stereo.mix(&[1.0], Some(0));
    assert_eq!(stereo.frames(), &[1.0, 1.0]);
}

#[test]
fn buffer_sink_pcm_s16le_and_lifecycle() {
    let mut sink = BufferSink::new(2, 1);
    sink.mix(&[1.0, -1.0], Some(0));
    let pcm = sink.to_pcm_s16le();
    let mut expected = Vec::new();
    expected.extend_from_slice(&32767i16.to_le_bytes());
    expected.extend_from_slice(&(-32767i16).to_le_bytes());
    assert_eq!(pcm, expected);
    sink.stop();
    sink.close();
}
