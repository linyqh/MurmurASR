import i18n from "../i18n";

/**
 * The backend reports errors as English strings. Known messages are mapped to
 * translation keys in the `errors` namespace; named groups become parameters,
 * and a `detail` group is itself translated (backend errors nest).
 * Unknown messages are shown as-is.
 */
const PATTERNS: [key: string, re: RegExp][] = [
  ["portUnavailable", /^Port (?<port>\d+) is unavailable: (?<detail>.*)$/],
  ["openAudio", /^Couldn't open the audio file: (?<detail>.*)$/],
  ["unsupportedFormat", /^Unsupported audio format: (?<detail>.*)$/],
  ["decodeFailed", /^Audio decoding failed: (?<detail>.*)$/],
  ["prepareDecoder", /^Couldn't prepare the ffmpeg decoder: (?<detail>.*)$/],
  ["noAudio", /^The file contains no audio$/],
  ["micUnavailable", /^Microphone unavailable: (?<detail>.*)$/],
  ["noAudioTrack", /^No audio track$/],
  ["unknownCodec", /^Unrecognized codec parameters$/],
  ["cancelled", /^Cancelled$/],
  ["captureExited", /^The capture thread exited unexpectedly$/],
  ["invalidDevice", /^Invalid device ID: (?<id>.*)$/],
  ["selectedMicDisconnected", /^The selected microphone was disconnected$/],
  ["noMic", /^No microphone found$/],
  ["sampleFormat", /^Unsupported sample format (?<format>.*)$/],
  ["subtitleLines", /^Subtitle lines must be 1 or 2$/],
  ["unknownModel", /^Unknown model: (?<id>.*)$/],
  ["helperModel", /^(?<name>.+) is a helper model and can't be used for recognition on its own$/],
  ["modelNotDownloaded", /^The model hasn't been downloaded yet$/],
  ["fileNotFound", /^File not found$/],
  ["loadModelFirst", /^Load a model first$/],
  ["transcriptNotFound", /^Transcript not found$/],
  ["engineLoad", /^Couldn't load the inference engine (?<path>.+?): (?<detail>.*)$/],
  ["engineSymbol", /^The inference engine is missing symbol (?<symbol>.*)$/],
  ["engineAbi", /^Inference engine ABI version is incompatible \(need (?<need>[\d.]+)\+, found (?<found>.*)\)$/],
  ["engineCall", /^(?<op>\S+) failed \((?<status>.*?)\): (?<detail>.*)$/],
  ["illegalNul", /^Argument contains an illegal NUL character$/],
  ["engineUnavailable", /^Inference engine unavailable: (?<detail>.*)$/],
  ["threadExited", /^The inference thread has exited$/],
  ["vadLoad", /^Couldn't load VAD: (?<detail>.*)$/],
  ["modelNoAsr", /^Model (?<family>.+) doesn't support speech recognition$/],
  ["loadModelInModels", /^Download and load a model in Models first$/],
  ["micDisconnected", /^The microphone was disconnected$/],
  ["noFfmpeg", /^No ffmpeg build is available for this platform$/],
  ["ffmpegChecksum", /^File verification failed \(SHA-256 mismatch\)\. Please try again$/],
  ["ffmpegStart", /^Couldn't start ffmpeg: (?<detail>.*)$/],
  ["modelsDir", /^Couldn't create the models folder: (?<detail>.*)$/],
  ["sourceChecksum", /^(?<label>.+?): file verification failed \(SHA-256 mismatch\)$/],
  ["incompleteDownload", /^Incomplete download \((?<downloaded>\d+)\/(?<size>\d+) bytes\)$/],
];

function translate(message: string): string {
  for (const [key, re] of PATTERNS) {
    const match = re.exec(message);
    if (!match) continue;
    const params = { ...match.groups };
    if (params.detail) params.detail = translate(params.detail);
    return i18n.t(`errors:${key}` as "errors:cancelled", { ...params, defaultValue: message });
  }
  return message;
}

export function errorMessage(e: unknown): string {
  return translate(e instanceof Error ? e.message : String(e));
}
