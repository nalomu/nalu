#!/usr/bin/env swift
import Foundation
import AVFoundation
import AudioToolbox

// MARK: - Standard MIDI File (SMF) Parser

enum MidiEventKind {
    case tempo(usPerQuarter: UInt32)
    case noteOn(channel: UInt8, note: UInt8, velocity: UInt8)
    case noteOff(channel: UInt8, note: UInt8)
    case controlChange(channel: UInt8, controller: UInt8, value: UInt8)
    case programChange(channel: UInt8, program: UInt8)
    case pitchBend(channel: UInt8, value: UInt16)
}

struct RawMidiEvent {
    let tick: Int
    let order: Int
    let kind: MidiEventKind
}

struct ScheduledMidiEvent {
    let sample: Int
    let order: Int
    let kind: MidiEventKind
}

struct ParsedMidi {
    let tpqn: Int
    let events: [RawMidiEvent]
}

enum MidiError: Error, CustomStringConvertible {
    case invalidFile(String)

    var description: String {
        switch self {
        case .invalidFile(let msg): return "MIDI parse error: \(msg)"
        }
    }
}

final class ByteReader {
    let bytes: [UInt8]
    var offset: Int = 0

    init(_ data: Data) {
        self.bytes = [UInt8](data)
    }

    var remaining: Int { bytes.count - offset }

    func readUInt8() throws -> UInt8 {
        guard offset < bytes.count else { throw MidiError.invalidFile("Unexpected EOF") }
        let b = bytes[offset]
        offset += 1
        return b
    }

    func readUInt16BE() throws -> UInt16 {
        let b0 = UInt16(try readUInt8())
        let b1 = UInt16(try readUInt8())
        return (b0 << 8) | b1
    }

    func readUInt32BE() throws -> UInt32 {
        let b0 = UInt32(try readUInt8())
        let b1 = UInt32(try readUInt8())
        let b2 = UInt32(try readUInt8())
        let b3 = UInt32(try readUInt8())
        return (b0 << 24) | (b1 << 16) | (b2 << 8) | b3
    }

    func readBytes(_ count: Int) throws -> [UInt8] {
        guard offset + count <= bytes.count else { throw MidiError.invalidFile("Unexpected EOF reading \(count) bytes") }
        let slice = Array(bytes[offset..<(offset + count)])
        offset += count
        return slice
    }

    func readVLQ() throws -> Int {
        var value = 0
        for _ in 0..<4 {
            let b = try readUInt8()
            value = (value << 7) | Int(b & 0x7F)
            if (b & 0x80) == 0 {
                return value
            }
        }
        throw MidiError.invalidFile("Invalid VLQ encoding")
    }
}

func parseMidiFile(at url: URL) throws -> ParsedMidi {
    let data = try Data(contentsOf: url)
    let reader = ByteReader(data)

    let headerTag = try String(bytes: reader.readBytes(4), encoding: .ascii)
    guard headerTag == "MThd" else { throw MidiError.invalidFile("Missing MThd header") }
    let headerLen = Int(try reader.readUInt32BE())
    guard headerLen >= 6 else { throw MidiError.invalidFile("Invalid MThd length") }
    let _ = try reader.readUInt16BE() // format (0 or 1)
    let numTracks = Int(try reader.readUInt16BE())
    let division = try reader.readUInt16BE()
    guard (division & 0x8000) == 0 else {
        throw MidiError.invalidFile("SMPTE division is not supported")
    }
    let tpqn = Int(division)
    if headerLen > 6 {
        _ = try reader.readBytes(headerLen - 6)
    }

    var allEvents: [RawMidiEvent] = []
    var globalOrder = 0

    for _ in 0..<numTracks {
        guard reader.remaining >= 8 else { break }
        let trackTag = try String(bytes: reader.readBytes(4), encoding: .ascii)
        let trackLen = Int(try reader.readUInt32BE())
        let trackEnd = reader.offset + trackLen
        if trackTag != "MTrk" {
            reader.offset = trackEnd
            continue
        }

        var tick = 0
        var runningStatus: UInt8 = 0

        while reader.offset < trackEnd {
            let delta = try reader.readVLQ()
            tick += delta

            let first = try reader.readUInt8()
            var status: UInt8
            var firstData: UInt8?

            if (first & 0x80) != 0 {
                status = first
                if status < 0xF0 {
                    runningStatus = status
                }
            } else {
                guard runningStatus != 0 else {
                    throw MidiError.invalidFile("Running status without prior status byte")
                }
                status = runningStatus
                firstData = first
            }

            if status == 0xFF {
                let metaType = try reader.readUInt8()
                let metaLen = try reader.readVLQ()
                let metaData = try reader.readBytes(metaLen)
                if metaType == 0x51 && metaLen == 3 {
                    let us = (UInt32(metaData[0]) << 16) | (UInt32(metaData[1]) << 8) | UInt32(metaData[2])
                    allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .tempo(usPerQuarter: us)))
                    globalOrder += 1
                } else if metaType == 0x2F {
                    reader.offset = trackEnd
                    break
                }
            } else if status == 0xF0 || status == 0xF7 {
                let sysexLen = try reader.readVLQ()
                _ = try reader.readBytes(sysexLen)
            } else {
                let msgType = status & 0xF0
                let channel = status & 0x0F
                let d1 = try firstData ?? reader.readUInt8()

                switch msgType {
                case 0x80:
                    let _ = try reader.readUInt8()
                    allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .noteOff(channel: channel, note: d1)))
                    globalOrder += 1
                case 0x90:
                    let vel = try reader.readUInt8()
                    if vel == 0 {
                        allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .noteOff(channel: channel, note: d1)))
                    } else {
                        allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .noteOn(channel: channel, note: d1, velocity: vel)))
                    }
                    globalOrder += 1
                case 0xA0:
                    let _ = try reader.readUInt8() // poly aftertouch
                case 0xB0:
                    let val = try reader.readUInt8()
                    allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .controlChange(channel: channel, controller: d1, value: val)))
                    globalOrder += 1
                case 0xC0:
                    allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .programChange(channel: channel, program: d1)))
                    globalOrder += 1
                case 0xD0:
                    break // channel pressure (1 data byte)
                case 0xE0:
                    let d2 = try reader.readUInt8()
                    let bend = UInt16(d1) | (UInt16(d2) << 7)
                    allEvents.append(RawMidiEvent(tick: tick, order: globalOrder, kind: .pitchBend(channel: channel, value: bend)))
                    globalOrder += 1
                default:
                    break
                }
            }
        }
        reader.offset = trackEnd
    }

    return ParsedMidi(tpqn: tpqn, events: allEvents)
}

func eventPriority(_ kind: MidiEventKind) -> Int {
    switch kind {
    case .tempo: return 0
    case .programChange: return 1
    case .controlChange: return 2
    case .pitchBend: return 2
    case .noteOff: return 3
    case .noteOn: return 4
    }
}

func scheduleEvents(_ parsed: ParsedMidi, sampleRate: Double) -> ([ScheduledMidiEvent], Double) {
    let sorted = parsed.events.sorted { a, b in
        if a.tick != b.tick { return a.tick < b.tick }
        let pa = eventPriority(a.kind)
        let pb = eventPriority(b.kind)
        if pa != pb { return pa < pb }
        return a.order < b.order
    }

    var currentTick = 0
    var currentSeconds = 0.0
    var usPerQuarter: Double = 500_000.0 // Default 120 BPM
    var scheduled: [ScheduledMidiEvent] = []
    var maxEventSeconds = 0.0

    for (idx, ev) in sorted.enumerated() {
        let deltaTicks = ev.tick - currentTick
        if deltaTicks > 0 {
            let deltaSec = (Double(deltaTicks) / Double(parsed.tpqn)) * (usPerQuarter / 1_000_000.0)
            currentSeconds += deltaSec
            currentTick = ev.tick
        }
        if case .tempo(let us) = ev.kind {
            usPerQuarter = Double(us)
            continue
        }
        let sample = Int(round(currentSeconds * sampleRate))
        scheduled.append(ScheduledMidiEvent(sample: sample, order: idx, kind: ev.kind))
        if currentSeconds > maxEventSeconds {
            maxEventSeconds = currentSeconds
        }
    }

    return (scheduled, maxEventSeconds)
}

// MARK: - Offline Audio Renderer

struct RenderJob: Decodable {
    let id: String
    let midi: String
    let wav: String
    let duration: Double?
    let reverb: Double?
    let fadeOut: Double?
    let peakDb: Double?
}

func renderMidiToWav(
    midiURL: URL,
    wavURL: URL,
    soundFontURL: URL,
    targetDuration: Double?,
    reverbMix: Float,
    fadeOutSeconds: Double,
    peakDb: Double
) throws {
    let sampleRate: Double = 44100.0
    let parsed = try parseMidiFile(at: midiURL)
    let (events, lastEventSec) = scheduleEvents(parsed, sampleRate: sampleRate)

    let totalDuration = targetDuration ?? (lastEventSec + 1.5)
    let totalSamples = Int(round(totalDuration * sampleRate))

    // Discover active channels and initial programs
    var activeChannels = Set<UInt8>()
    var initialProgram: [UInt8: UInt8] = [:]
    for ev in events {
        switch ev.kind {
        case .noteOn(let ch, _, _), .noteOff(let ch, _), .controlChange(let ch, _, _), .pitchBend(let ch, _):
            activeChannels.insert(ch)
        case .programChange(let ch, let prog):
            activeChannels.insert(ch)
            if initialProgram[ch] == nil && ev.sample == 0 {
                initialProgram[ch] = prog
            }
        case .tempo:
            break
        }
    }
    if activeChannels.isEmpty {
        activeChannels.insert(0)
    }

    let engine = AVAudioEngine()
    let subMixer = AVAudioMixerNode()
    let eq = AVAudioUnitEQ(numberOfBands: 2)
    let reverb = AVAudioUnitReverb()

    // Band 0: Gentle high-pass at 45 Hz to keep the mix clean
    eq.bands[0].filterType = .highPass
    eq.bands[0].frequency = 45
    eq.bands[0].bypass = false

    // Band 1: Subtle warm air presence (+1.2 dB @ 3.5 kHz)
    eq.bands[1].filterType = .parametric
    eq.bands[1].frequency = 3500
    eq.bands[1].bandwidth = 1.4
    eq.bands[1].gain = 1.2
    eq.bands[1].bypass = false

    reverb.loadFactoryPreset(.mediumHall)
    reverb.wetDryMix = reverbMix

    engine.attach(subMixer)
    engine.attach(eq)
    engine.attach(reverb)

    let format = AVAudioFormat(standardFormatWithSampleRate: sampleRate, channels: 2)!

    var samplers: [UInt8: AVAudioUnitSampler] = [:]
    var loadedProgram: [UInt8: UInt8] = [:]

    for ch in activeChannels.sorted() {
        let sampler = AVAudioUnitSampler()
        engine.attach(sampler)
        engine.connect(sampler, to: subMixer, format: format)
        samplers[ch] = sampler
    }

    engine.connect(subMixer, to: eq, format: format)
    engine.connect(eq, to: reverb, format: format)
    engine.connect(reverb, to: engine.mainMixerNode, format: format)

    let maxFrames: AVAudioFrameCount = 512
    try engine.enableManualRenderingMode(.offline, format: format, maximumFrameCount: maxFrames)
    try engine.start()

    func loadInstrument(for channel: UInt8, program: UInt8) throws {
        guard let sampler = samplers[channel] else { return }
        if loadedProgram[channel] == program { return }
        let bankMSB: UInt8 = (channel == 9)
            ? UInt8(kAUSampler_DefaultPercussionBankMSB)
            : UInt8(kAUSampler_DefaultMelodicBankMSB)
        try sampler.loadSoundBankInstrument(
            at: soundFontURL,
            program: program,
            bankMSB: bankMSB,
            bankLSB: UInt8(kAUSampler_DefaultBankLSB)
        )
        loadedProgram[channel] = program
    }

    for ch in activeChannels.sorted() {
        try loadInstrument(for: ch, program: initialProgram[ch] ?? 0)
    }

    // Pre-allocate stereo float buffers
    var left = [Float]()
    var right = [Float]()
    left.reserveCapacity(totalSamples)
    right.reserveCapacity(totalSamples)

    guard let buffer = AVAudioPCMBuffer(pcmFormat: engine.manualRenderingFormat, frameCapacity: maxFrames) else {
        throw MidiError.invalidFile("Failed to allocate PCM buffer")
    }

    // Small 12ms pre-roll for notes so initial attack transients are never softened by sample-0 fade-in
    let leadInSamples = Int(round(0.012 * sampleRate))
    let shiftedEvents = events.map { ev -> ScheduledMidiEvent in
        switch ev.kind {
        case .programChange, .controlChange:
            if ev.sample == 0 { return ev }
            return ScheduledMidiEvent(sample: ev.sample + leadInSamples, order: ev.order, kind: ev.kind)
        default:
            return ScheduledMidiEvent(sample: ev.sample + leadInSamples, order: ev.order, kind: ev.kind)
        }
    }

    var currentSample = 0
    var eventIdx = 0

    while currentSample < totalSamples {
        while eventIdx < shiftedEvents.count && shiftedEvents[eventIdx].sample <= currentSample {
            let ev = shiftedEvents[eventIdx]
            eventIdx += 1
            switch ev.kind {
            case .programChange(let ch, let prog):
                try loadInstrument(for: ch, program: prog)
            case .controlChange(let ch, let ctrl, let val):
                samplers[ch]?.sendController(ctrl, withValue: val, onChannel: ch)
            case .noteOn(let ch, let note, let vel):
                samplers[ch]?.startNote(note, withVelocity: vel, onChannel: ch)
            case .noteOff(let ch, let note):
                samplers[ch]?.stopNote(note, onChannel: ch)
            case .pitchBend(let ch, let value):
                samplers[ch]?.sendPitchBend(value, onChannel: ch)
            case .tempo:
                break
            }
        }

        let nextEventSample = eventIdx < shiftedEvents.count ? shiftedEvents[eventIdx].sample : totalSamples
        let remaining = totalSamples - currentSample
        let untilNext = max(1, nextEventSample - currentSample)
        let frames = min(Int(maxFrames), min(remaining, untilNext))

        let status = try engine.renderOffline(AVAudioFrameCount(frames), to: buffer)
        guard status == .success, let chData = buffer.floatChannelData else {
            throw MidiError.invalidFile("Offline render failed with status \(status.rawValue)")
        }

        let rendered = Int(buffer.frameLength)
        for i in 0..<rendered {
            left.append(chData[0][i])
            right.append(chData[1][i])
        }
        currentSample += rendered
    }

    engine.stop()

    let count = min(left.count, right.count)
    guard count > 0 else {
        throw MidiError.invalidFile("Rendered zero samples")
    }

    // Remove DC offset
    var sumL: Double = 0
    var sumR: Double = 0
    for i in 0..<count {
        sumL += Double(left[i])
        sumR += Double(right[i])
    }
    let dcL = Float(sumL / Double(count))
    let dcR = Float(sumR / Double(count))
    for i in 0..<count {
        left[i] -= dcL
        right[i] -= dcR
    }

    // Apply click-free 4ms fade-in and smooth cosine fade-out
    let fadeInSamples = min(count, Int(round(0.004 * sampleRate)))
    if fadeInSamples > 1 {
        for i in 0..<fadeInSamples {
            let t = Double(i) / Double(fadeInSamples)
            let gain = Float(0.5 - 0.5 * cos(Double.pi * t))
            left[i] *= gain
            right[i] *= gain
        }
    }

    let fadeOutSamples = min(count, Int(round(fadeOutSeconds * sampleRate)))
    if fadeOutSamples > 1 {
        let start = count - fadeOutSamples
        for i in 0..<fadeOutSamples {
            let t = Double(i) / Double(fadeOutSamples - 1)
            let gain = Float(0.5 + 0.5 * cos(Double.pi * t))
            left[start + i] *= gain
            right[start + i] *= gain
        }
    }

    // Peak normalize to target dBFS
    var peak: Float = 0
    for i in 0..<count {
        peak = max(peak, abs(left[i]), abs(right[i]))
    }
    let targetPeak = Float(pow(10.0, peakDb / 20.0))
    let gain: Float = peak > 1e-6 ? (targetPeak / peak) : 1.0
    for i in 0..<count {
        left[i] = max(-1.0, min(1.0, left[i] * gain))
        right[i] = max(-1.0, min(1.0, right[i] * gain))
    }

    // Write 16-bit Stereo PCM WAV (44.1 kHz) with lightweight TPDF dithering
    try writeStereoWav16(url: wavURL, sampleRate: Int(sampleRate), left: left, right: right, count: count)

    // Compute RMS for diagnostics
    var sqSum: Double = 0
    for i in 0..<count {
        sqSum += Double(left[i]) * Double(left[i]) + Double(right[i]) * Double(right[i])
    }
    let rmsDb = 20.0 * log10(max(1e-9, sqrt(sqSum / Double(count * 2))))
    let finalPeakDb = 20.0 * log10(max(1e-9, Double(peak * gain)))
    print(String(format: "[render-midi] %@ -> %@ (%.2fs, stereo 44.1kHz, peak %.1f dBFS, RMS %.1f dBFS)",
                 midiURL.lastPathComponent, wavURL.lastPathComponent, Double(count) / sampleRate, finalPeakDb, rmsDb))
}

func writeStereoWav16(url: URL, sampleRate: Int, left: [Float], right: [Float], count: Int) throws {
    let numChannels: UInt16 = 2
    let bitsPerSample: UInt16 = 16
    let blockAlign: UInt16 = numChannels * (bitsPerSample / 8)
    let byteRate: UInt32 = UInt32(sampleRate) * UInt32(blockAlign)
    let dataSize: UInt32 = UInt32(count) * UInt32(blockAlign)
    let riffSize: UInt32 = 36 + dataSize

    var data = Data()
    data.reserveCapacity(44 + Int(dataSize))

    func appendASCII(_ s: String) { data.append(contentsOf: s.utf8) }
    func appendLE(_ v: UInt16) { var x = v.littleEndian; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
    func appendLE(_ v: UInt32) { var x = v.littleEndian; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }
    func appendLE(_ v: Int16)  { var x = v.littleEndian; withUnsafeBytes(of: &x) { data.append(contentsOf: $0) } }

    appendASCII("RIFF")
    appendLE(riffSize)
    appendASCII("WAVE")
    appendASCII("fmt ")
    appendLE(UInt32(16))
    appendLE(UInt16(1)) // PCM
    appendLE(numChannels)
    appendLE(UInt32(sampleRate))
    appendLE(byteRate)
    appendLE(blockAlign)
    appendLE(bitsPerSample)
    appendASCII("data")
    appendLE(dataSize)

    // Deterministic LCG for TPDF dithering (reproducible builds)
    var rngState: UInt64 = 0x5EED_CAFE_1234_5678
    @inline(__always) func nextRand() -> Float {
        rngState = rngState &* 6364136223846793005 &+ 1442695040888963407
        return Float((rngState >> 33) & 0x7FFF_FFFF) / Float(0x8000_0000)
    }

    @inline(__always) func quantize(_ sample: Float) -> Int16 {
        let scaled = sample * 32767.0
        // Keep exact zero at silence boundaries
        if abs(scaled) < 1e-4 { return 0 }
        let dither = (nextRand() - nextRand()) * 0.5
        let clamped = max(-32768.0, min(32767.0, round(scaled + dither)))
        return Int16(clamped)
    }

    for i in 0..<count {
        appendLE(quantize(left[i]))
        appendLE(quantize(right[i]))
    }

    try FileManager.default.createDirectory(at: url.deletingLastPathComponent(), withIntermediateDirectories: true)
    try data.write(to: url, options: .atomic)
}

// MARK: - CLI Entry Point

let defaultSoundFont = "/System/Library/Components/CoreAudio.component/Contents/Resources/gs_instruments.dls"

func argValue(_ flag: String) -> String? {
    let args = CommandLine.arguments
    guard let idx = args.firstIndex(of: flag), idx + 1 < args.count else { return nil }
    return args[idx + 1]
}

do {
    let soundFontPath = argValue("--soundfont") ?? defaultSoundFont
    let soundFontURL = URL(fileURLWithPath: soundFontPath)
    guard FileManager.default.fileExists(atPath: soundFontPath) else {
        fputs("SoundFont not found: \(soundFontPath)\n", stderr)
        exit(1)
    }

    if let manifestPath = argValue("--manifest") {
        let repoRoot = URL(fileURLWithPath: argValue("--root") ?? FileManager.default.currentDirectoryPath)
        let manifestData = try Data(contentsOf: URL(fileURLWithPath: manifestPath))
        let jobs = try JSONDecoder().decode([RenderJob].self, from: manifestData)
        for job in jobs {
            let midiURL = job.midi.hasPrefix("/") ? URL(fileURLWithPath: job.midi) : repoRoot.appendingPathComponent(job.midi)
            let wavURL = job.wav.hasPrefix("/") ? URL(fileURLWithPath: job.wav) : repoRoot.appendingPathComponent(job.wav)
            try renderMidiToWav(
                midiURL: midiURL,
                wavURL: wavURL,
                soundFontURL: soundFontURL,
                targetDuration: job.duration,
                reverbMix: Float(job.reverb ?? 24.0),
                fadeOutSeconds: job.fadeOut ?? 0.30,
                peakDb: job.peakDb ?? -1.5
            )
        }
    } else if let midiPath = argValue("--midi"), let outPath = argValue("--out") {
        try renderMidiToWav(
            midiURL: URL(fileURLWithPath: midiPath),
            wavURL: URL(fileURLWithPath: outPath),
            soundFontURL: soundFontURL,
            targetDuration: argValue("--duration").flatMap(Double.init),
            reverbMix: Float(argValue("--reverb").flatMap(Double.init) ?? 24.0),
            fadeOutSeconds: argValue("--fade-out").flatMap(Double.init) ?? 0.30,
            peakDb: argValue("--peak-db").flatMap(Double.init) ?? -1.5
        )
    } else {
        print("""
        Usage:
          swift render-midi.swift --manifest scripts/sounds/midi/manifest.json [--root .] [--soundfont path.dls]
          swift render-midi.swift --midi input.mid --out output.wav [--duration 4.0] [--reverb 24] [--fade-out 0.3] [--peak-db -1.5]
        """)
        exit(1)
    }
} catch {
    fputs("Error: \(error)\n", stderr)
    exit(1)
}
