use lib_kman::libloading;
use lib_kman::make_lib;
use libloading::Library;
use libloading::Symbol;

pub const OPUS_OK: i32 = 0;
pub const OPUS_BAD_ARG: i32 = -1;
pub const OPUS_BUFFER_TO_SMALL: i32 = -2;
pub const OPUS_INTERNAL_ERROR: i32 = -3;
pub const OPUS_INVALID_PACKET: i32 = -4;
pub const OPUS_UNIMPLEMENTED: i32 = -5;
pub const OPUS_INVALID_STATE: i32 = -6;
pub const OPUS_ALLOC_FAIL: i32 = -7;

pub const OPUS_SET_APPLICATION_REQUEST: i32 = 4000;
pub const OPUS_GET_APPLICATION_REQUEST: i32 = 4001;
pub const OPUS_SET_BITRATE_REQUEST: i32 = 4002;
pub const OPUS_GET_BITRATE_REQUEST: i32 = 4003;
pub const OPUS_SET_MAX_BANDWIDTH_REQUEST: i32 = 4004;
pub const OPUS_GET_MAX_BANDWIDTH_REQUEST: i32 = 4005;
pub const OPUS_SET_VBR_REQUEST: i32 = 4006;
pub const OPUS_GET_VBR_REQUEST: i32 = 4007;
pub const OPUS_SET_BANDWIDTH_REQUEST: i32 = 4008;
pub const OPUS_GET_BANDWIDTH_REQUEST: i32 = 4009;
pub const OPUS_SET_COMPLEXITY_REQUEST: i32 = 4010;
pub const OPUS_GET_COMPLEXITY_REQUEST: i32 = 4011;
pub const OPUS_SET_INBAND_FEC_REQUEST: i32 = 4012;
pub const OPUS_GET_INBAND_FEC_REQUEST: i32 = 4013;
pub const OPUS_SET_PACKET_LOSS_PERC_REQUEST: i32 = 4014;
pub const OPUS_GET_PACKET_LOSS_PERC_REQUEST: i32 = 4015;
pub const OPUS_SET_DTX_REQUEST: i32 = 4016;
pub const OPUS_GET_DTX_REQUEST: i32 = 4017;
pub const OPUS_SET_VBR_CONSTRAINT_REQUEST: i32 = 4020;
pub const OPUS_GET_VBR_CONSTRAINT_REQUEST: i32 = 4021;
pub const OPUS_SET_FORCE_CHANNELS_REQUEST: i32 = 4022;
pub const OPUS_GET_FORCE_CHANNELS_REQUEST: i32 = 4023;
pub const OPUS_SET_SIGNAL_REQUEST: i32 = 4024;
pub const OPUS_GET_SIGNAL_REQUEST: i32 = 4025;
pub const OPUS_GET_LOOKAHEAD_REQUEST: i32 = 4027;
pub const OPUS_GET_SAMPLE_RATE_REQUEST: i32 = 4029;
pub const OPUS_GET_FINAL_RANGE_REQUEST: i32 = 4031;
pub const OPUS_GET_PITCH_REQUEST: i32 = 4033;
pub const OPUS_SET_GAIN_REQUEST: i32 = 4034;
pub const OPUS_GET_GAIN_REQUEST: i32 = 4045;
pub const OPUS_SET_LSB_DEPTH_REQUEST: i32 = 4036;
pub const OPUS_GET_LSB_DEPTH_REQUEST: i32 = 4037;
pub const OPUS_GET_LAST_PACKET_DURATION_REQUEST: i32 = 4039;
pub const OPUS_SET_EXPERT_FRAME_DURATION_REQUEST: i32 = 4040;
pub const OPUS_GET_EXPERT_FRAME_DURATION_REQUEST: i32 = 4041;
pub const OPUS_SET_PREDICTION_DISABLED_REQUEST: i32 = 4042;
pub const OPUS_GET_PREDICTION_DISABLED_REQUEST: i32 = 4043;
pub const OPUS_SET_PHASE_INVERSION_DISABLED_REQUEST: i32 = 4046;
pub const OPUS_GET_PHASE_INVERSION_DISABLED_REQUEST: i32 = 4047;
pub const OPUS_GET_IN_DTX_REQUEST: i32 = 4049;
pub const OPUS_SET_DRED_DURATION_REQUEST: i32 = 4050;
pub const OPUS_GET_DRED_DURATION_REQUEST: i32 = 4051;
pub const OPUS_SET_DNN_BLOB_REQUEST: i32 = 4052;

pub const OPUS_AUTO: i32 = -1000;
pub const OPUS_BITRATE_MAX: i32 = -1;
pub const OPUS_APPLICATION_VOIP: i32 = 2048;
pub const OPUS_APPLICATION_AUDIO: i32 = 2049;
pub const OPUS_APPLICATION_RESTRICTED_LOWDELAY: i32 = 2051;
pub const OPUS_SIGNAL_VOICE: i32 = 3001;
pub const OPUS_SIGNAL_MUSIC: i32 = 3002;
pub const OPUS_BANDWIDTH_NARROWBAND: i32 = 1101;
pub const OPUS_BANDWIDTH_MEDIUMBAND: i32 = 1102;
pub const OPUS_BANDWIDTH_WIDEBAND: i32 = 1103;
pub const OPUS_BANDWIDTH_SUPERWIDEBAND: i32 = 1104;
pub const OPUS_BANDWIDTH_FULLBAND: i32 = 1105;
pub const OPUS_FRAMESIZE_ARG: i32 = 5000;
pub const OPUS_FRAMESIZE_2_5_MS: i32 = 5001;
pub const OPUS_FRAMESIZE_5_MS: i32 = 5002;
pub const OPUS_FRAMESIZE_10_MS: i32 = 5003;
pub const OPUS_FRAMESIZE_20_MS: i32 = 5004;
pub const OPUS_FRAMESIZE_40_MS: i32 = 5005;
pub const OPUS_FRAMESIZE_60_MS: i32 = 5006;
pub const OPUS_FRAMESIZE_80_MS: i32 = 5007;
pub const OPUS_FRAMESIZE_100_MS: i32 = 5008;
pub const OPUS_FRAMESIZE_120_MS: i32 = 5009;

pub enum RawOpusEncoder {}
#[derive(Clone, Copy)]
pub struct OpusEncoder(*mut RawOpusEncoder);

pub enum RawOpusDecoder {}
#[derive(Clone, Copy)]
pub struct OpusDecoder(*mut RawOpusDecoder);

unsafe impl Send for OpusEncoder {}
unsafe impl Sync for OpusEncoder {}
unsafe impl Send for OpusDecoder {}
unsafe impl Sync for OpusDecoder {}

make_lib! {
    pub struct OpusLibSys{
        opus_encoder_get_size: extern "C" fn(i32) -> i32;
        opus_encoder_init: extern "C" fn(*mut RawOpusEncoder, i32, i32, i32) -> i32;
        opus_encode: extern "C" fn(*mut RawOpusEncoder, *const i16, i32, *mut u8, i32) -> i32;
        opus_encode_float: extern "C" fn(*mut RawOpusEncoder, *const f32, i32, *mut u8, i32) -> i32;
        opus_encoder_destroy: extern "C" fn(*mut RawOpusEncoder);
        opus_encoder_ctl: extern "C" fn(*mut RawOpusEncoder, i32, ...) -> i32;

        opus_decoder_get_size: extern "C" fn(i32) -> i32;
        opus_decoder_init: extern "C" fn (*mut RawOpusDecoder, i32, i32) -> i32;
        opus_decode: extern "C" fn(*mut RawOpusDecoder, *const u8, i32, *mut i16, i32, i32) -> i32;
        opus_decode_float: extern "C" fn(*mut RawOpusDecoder, *const u8, i32, *mut f32, i32, i32) -> i32;
        opus_decoder_destroy: extern "C" fn(*mut RawOpusDecoder);
        opus_decoder_ctl: extern "C" fn(*mut RawOpusDecoder, i32, ...) -> i32;
    }
}

impl OpusLibSys {
    pub unsafe fn new() -> Result<Self, libloading::Error> {
        Self::with_library(Library::new(lib_kman::libloading::library_filename(
            "opus",
        ))?)
    }

    /** Gets the size of an <code>OpusEncoder</code> structure.
     * @param[in] channels <tt>int</tt>: Number of channels.
     *                                   This must be 1 or 2.
     * @returns The size in bytes.
     */
    pub unsafe fn opus_encoder_get_size(&self, channels: i32) -> i32 {
        (self._opus_encoder_get_size)(channels)
    }

    /** Allocates and initializes an encoder state.
     * There are three coding modes:
     *
     * @ref OPUS_APPLICATION_VOIP gives best quality at a given bitrate for voice
     *    signals. It enhances the  input signal by high-pass filtering and
     *    emphasizing formants and harmonics. Optionally  it includes in-band
     *    forward error correction to protect against packet loss. Use this
     *    mode for typical VoIP applications. Because of the enhancement,
     *    even at high bitrates the output may sound different from the input.
     *
     * @ref OPUS_APPLICATION_AUDIO gives best quality at a given bitrate for most
     *    non-voice signals like music. Use this mode for music and mixed
     *    (music/voice) content, broadcast, and applications requiring less
     *    than 15 ms of coding delay.
     *
     * @ref OPUS_APPLICATION_RESTRICTED_LOWDELAY configures low-delay mode that
     *    disables the speech-optimized mode in exchange for slightly reduced delay.
     *    This mode can only be set on an newly initialized or freshly reset encoder
     *    because it changes the codec delay.
     *
     * This is useful when the caller knows that the speech-optimized modes will not be needed (use with caution).
     * @param [in] Fs <tt>opus_int32</tt>: Sampling rate of input signal (Hz)
     *                                     This must be one of 8000, 12000, 16000,
     *                                     24000, or 48000.
     * @param [in] channels <tt>int</tt>: Number of channels (1 or 2) in input signal
     * @param [in] application <tt>int</tt>: Coding mode (one of @ref OPUS_APPLICATION_VOIP, @ref OPUS_APPLICATION_AUDIO, or @ref OPUS_APPLICATION_RESTRICTED_LOWDELAY)
     * @param [out] error <tt>int*</tt>: @ref opus_errorcodes
     * @note Regardless of the sampling rate and number channels selected, the Opus encoder
     * can switch to a lower audio bandwidth or number of channels if the bitrate
     * selected is too low. This also means that it is safe to always use 48 kHz stereo input
     * and let the encoder optimize the encoding.
     */
    pub unsafe fn opus_encoder_create(
        &self,
        fs: i32,
        channels: i32,
        application: i32,
        error: *mut i32,
    ) -> OpusEncoder {
        let size = (self._opus_encoder_get_size)(channels) as usize;
        let encoder = std::alloc::alloc(std::alloc::Layout::from_size_align(size, 8).unwrap())
            as *mut RawOpusEncoder;
        *error = (self._opus_encoder_init)(encoder, fs, channels, application);
        OpusEncoder(encoder)
    }

    /** Initializes a previously allocated encoder state
     * The memory pointed to by st must be at least the size returned by opus_encoder_get_size().
     * This is intended for applications which use their own allocator instead of malloc.
     * @see opus_encoder_create(),opus_encoder_get_size()
     * To reset a previously initialized state, use the #OPUS_RESET_STATE CTL.
     * @param [in] st <tt>OpusEncoder*</tt>: Encoder state
     * @param [in] Fs <tt>opus_int32</tt>: Sampling rate of input signal (Hz)
     *                                      This must be one of 8000, 12000, 16000,
     *                                      24000, or 48000.
     * @param [in] channels <tt>int</tt>: Number of channels (1 or 2) in input signal
     * @param [in] application <tt>int</tt>: Coding mode (one of OPUS_APPLICATION_VOIP, OPUS_APPLICATION_AUDIO, or OPUS_APPLICATION_RESTRICTED_LOWDELAY)
     * @retval #OPUS_OK Success or @ref opus_errorcodes
     */
    pub unsafe fn opus_encoder_init(
        &self,
        st: &mut OpusEncoder,
        fs: i32,
        channels: i32,
        application: i32,
    ) -> i32 {
        (self._opus_encoder_init)(st.0, fs, channels, application)
    }

    /** Encodes an Opus frame.
     * @param [in] st <tt>OpusEncoder*</tt>: Encoder state
     * @param [in] pcm <tt>opus_int16*</tt>: Input signal (interleaved if 2 channels). length is frame_size*channels*sizeof(opus_int16)
     * @param [in] frame_size <tt>int</tt>: Number of samples per channel in the
     *                                      input signal.
     *                                      This must be an Opus frame size for
     *                                      the encoder's sampling rate.
     *                                      For example, at 48 kHz the permitted
     *                                      values are 120, 240, 480, 960, 1920,
     *                                      and 2880.
     *                                      Passing in a duration of less than
     *                                      10 ms (480 samples at 48 kHz) will
     *                                      prevent the encoder from using the LPC
     *                                      or hybrid modes.
     * @param [out] data <tt>unsigned char*</tt>: Output payload.
     *                                            This must contain storage for at
     *                                            least \a max_data_bytes.
     * @param [in] max_data_bytes <tt>opus_int32</tt>: Size of the allocated
     *                                                 memory for the output
     *                                                 payload. This may be
     *                                                 used to impose an upper limit on
     *                                                 the instant bitrate, but should
     *                                                 not be used as the only bitrate
     *                                                 control. Use #OPUS_SET_BITRATE to
     *                                                 control the bitrate.
     * @returns The length of the encoded packet (in bytes) on success or a
     *          negative error code (see @ref opus_errorcodes) on failure.
     */
    pub unsafe fn opus_encode(
        &self,
        st: &mut OpusEncoder,
        pcm: *const i16,
        frame_size: i32,
        data: *mut u8,
        max_data_bytes: i32,
    ) -> i32 {
        (self._opus_encode)(st.0, pcm, frame_size, data, max_data_bytes)
    }

    /** Encodes an Opus frame from floating point input.
     * @param [in] st <tt>OpusEncoder*</tt>: Encoder state
     * @param [in] pcm <tt>float*</tt>: Input in float format (interleaved if 2 channels), with a normal range of +/-1.0.
     *          Samples with a range beyond +/-1.0 are supported but will
     *          be clipped by decoders using the integer API and should
     *          only be used if it is known that the far end supports
     *          extended dynamic range.
     *          length is frame_size*channels*sizeof(float)
     * @param [in] frame_size <tt>int</tt>: Number of samples per channel in the
     *                                      input signal.
     *                                      This must be an Opus frame size for
     *                                      the encoder's sampling rate.
     *                                      For example, at 48 kHz the permitted
     *                                      values are 120, 240, 480, 960, 1920,
     *                                      and 2880.
     *                                      Passing in a duration of less than
     *                                      10 ms (480 samples at 48 kHz) will
     *                                      prevent the encoder from using the LPC
     *                                      or hybrid modes.
     * @param [out] data <tt>unsigned char*</tt>: Output payload.
     *                                            This must contain storage for at
     *                                            least \a max_data_bytes.
     * @param [in] max_data_bytes <tt>opus_int32</tt>: Size of the allocated
     *                                                 memory for the output
     *                                                 payload. This may be
     *                                                 used to impose an upper limit on
     *                                                 the instant bitrate, but should
     *                                                 not be used as the only bitrate
     *                                                 control. Use #OPUS_SET_BITRATE to
     *                                                 control the bitrate.
     * @returns The length of the encoded packet (in bytes) on success or a
     *          negative error code (see @ref opus_errorcodes) on failure.
     */
    pub unsafe fn opus_encode_float(
        &self,
        st: &mut OpusEncoder,
        pcm: *const f32,
        frame_size: i32,
        data: *mut u8,
        max_data_bytes: i32,
    ) -> i32 {
        (self._opus_encode_float)(st.0, pcm, frame_size, data, max_data_bytes)
    }

    pub unsafe fn opus_encoder_set_bitrate(&self, st: &mut OpusEncoder, bitrate: i32) {
        (self._opus_encoder_ctl)(st.0, OPUS_SET_BITRATE_REQUEST, bitrate);
    }

    pub unsafe fn opus_encoder_set_application(&self, st: &mut OpusEncoder, application: i32) {
        (self._opus_encoder_ctl)(st.0, OPUS_SET_APPLICATION_REQUEST, application);
    }

    /// frame_size needs to be: OPUS_FRAMESIZE
    pub unsafe fn opus_encoder_set_expect_frame_duration(
        &self,
        st: &mut OpusEncoder,
        frame_size: i32,
    ) {
        (self._opus_encoder_ctl)(st.0, OPUS_SET_EXPERT_FRAME_DURATION_REQUEST, frame_size);
    }

    /** Frees an <code>OpusEncoder</code> allocated by opus_encoder_create().
     * @param[in] st <tt>OpusEncoder*</tt>: State to be freed.
     */
    pub unsafe fn opus_encoder_destroy(&self, st: &mut OpusEncoder) {
        (self._opus_encoder_destroy)(st.0);
        st.0 = std::ptr::null_mut();
    }

    /** Allocates and initializes a decoder state.
     * @param [in] Fs <tt>opus_int32</tt>: Sample rate to decode at (Hz).
     *                                     This must be one of 8000, 12000, 16000,
     *                                     24000, or 48000.
     * @param [in] channels <tt>int</tt>: Number of channels (1 or 2) to decode
     * @param [out] error <tt>int*</tt>: #OPUS_OK Success or @ref opus_errorcodes
     *
     * Internally Opus stores data at 48000 Hz, so that should be the default
     * value for Fs. However, the decoder can efficiently decode to buffers
     * at 8, 12, 16, and 24 kHz so if for some reason the caller cannot use
     * data at the full sample rate, or knows the compressed data doesn't
     * use the full frequency range, it can request decoding at a reduced
     * rate. Likewise, the decoder is capable of filling in either mono or
     * interleaved stereo pcm buffers, at the caller's request.
     */
    pub unsafe fn opus_decoder_create(
        &self,
        fs: i32,
        channels: i32,
        error: *mut i32,
    ) -> OpusDecoder {
        let size = (self._opus_decoder_get_size)(channels) as usize;
        let decoder = std::alloc::alloc(std::alloc::Layout::from_size_align(size, 8).unwrap())
            as *mut RawOpusDecoder;
        *error = (self._opus_decoder_init)(decoder, fs, channels);
        OpusDecoder(decoder)
    }

    /** Decode an Opus packet.
     * @param [in] st <tt>OpusDecoder*</tt>: Decoder state
     * @param [in] data <tt>char*</tt>: Input payload. Use a NULL pointer to indicate packet loss
     * @param [in] len <tt>opus_int32</tt>: Number of bytes in payload*
     * @param [out] pcm <tt>opus_int16*</tt>: Output signal (interleaved if 2 channels). length
     *  is frame_size*channels*sizeof(opus_int16)
     * @param [in] frame_size Number of samples per channel of available space in \a pcm.
     *  If this is less than the maximum packet duration (120ms; 5760 for 48kHz), this function will
     *  not be capable of decoding some packets. In the case of PLC (data==NULL) or FEC (decode_fec=1),
     *  then frame_size needs to be exactly the duration of audio that is missing, otherwise the
     *  decoder will not be in the optimal state to decode the next incoming packet. For the PLC and
     *  FEC cases, frame_size <b>must</b> be a multiple of 2.5 ms.
     * @param [in] decode_fec <tt>int</tt>: Flag (0 or 1) to request that any in-band forward error correction data be
     *  decoded. If no such data is available, the frame is decoded as if it were lost.
     * @returns Number of decoded samples or @ref opus_errorcodes
     */
    pub unsafe fn opus_decode(
        &self,
        st: &mut OpusDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut i16,
        frame_size: i32,
        decode_fec: i32,
    ) -> i32 {
        (self._opus_decode)(st.0, data, len, pcm, frame_size, decode_fec)
    }

    /** Decode an Opus packet with floating point output.
     * @param [in] st <tt>OpusDecoder*</tt>: Decoder state
     * @param [in] data <tt>char*</tt>: Input payload. Use a NULL pointer to indicate packet loss
     * @param [in] len <tt>opus_int32</tt>: Number of bytes in payload
     * @param [out] pcm <tt>float*</tt>: Output signal (interleaved if 2 channels). length
     *  is frame_size*channels*sizeof(float)
     * @param [in] frame_size Number of samples per channel of available space in \a pcm.
     *  If this is less than the maximum packet duration (120ms; 5760 for 48kHz), this function will
     *  not be capable of decoding some packets. In the case of PLC (data==NULL) or FEC (decode_fec=1),
     *  then frame_size needs to be exactly the duration of audio that is missing, otherwise the
     *  decoder will not be in the optimal state to decode the next incoming packet. For the PLC and
     *  FEC cases, frame_size <b>must</b> be a multiple of 2.5 ms.
     * @param [in] decode_fec <tt>int</tt>: Flag (0 or 1) to request that any in-band forward error correction data be
     *  decoded. If no such data is available the frame is decoded as if it were lost.
     * @returns Number of decoded samples or @ref opus_errorcodes
     */
    pub unsafe fn opus_decode_float(
        &self,
        st: &mut OpusDecoder,
        data: *const u8,
        len: i32,
        pcm: *mut f32,
        frame_size: i32,
        decode_fec: i32,
    ) -> i32 {
        (self._opus_decode_float)(st.0, data, len, pcm, frame_size, decode_fec)
    }

    pub unsafe fn opus_decoder_destroy(&self, st: &mut OpusDecoder) {
        (self._opus_decoder_destroy)(st.0);
        st.0 = std::ptr::null_mut();
    }
}
