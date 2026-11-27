#!/usr/bin/env python3
"""
Generate Standard MIDI Files (SMF Type 1) for Nalu's built-in alert sounds.

Outputs:
  scripts/sounds/midi/nalu-gentle-bell.mid  (Pomodoro start — serene celesta/vibe/harp chime)
  scripts/sounds/midi/nalu-warm-chime.mid   (Alarm — seamless 2-bar / 4.0s warm acoustic loop)
  scripts/sounds/midi/nalu-soft-rise.mid    (Pomodoro end — sunrise arpeggio & radiant resolve)

Zero external dependencies — uses Python standard library only.
"""

from __future__ import annotations

import json
import os
import struct
from dataclasses import dataclass, field
from pathlib import Path
from typing import List, Tuple

TPQN = 480  # Ticks per quarter note

# General MIDI Program Numbers (0-indexed)
GM_ELECTRIC_PIANO_1 = 4   # Rhodes
GM_ELECTRIC_PIANO_2 = 5   # Chorused / Bell EP
GM_CELESTA = 8
GM_GLOCKENSPIEL = 9
GM_MUSIC_BOX = 10
GM_VIBRAPHONE = 11
GM_MARIMBA = 12
GM_ORCHESTRAL_HARP = 46
GM_WARM_PAD = 89

NOTE_OFFSETS = {
    "C": 0, "C#": 1, "Db": 1,
    "D": 2, "D#": 3, "Eb": 3,
    "E": 4,
    "F": 5, "F#": 6, "Gb": 6,
    "G": 7, "G#": 8, "Ab": 8,
    "A": 9, "A#": 10, "Bb": 10,
    "B": 11,
}


def note(name: str) -> int:
    """Convert scientific pitch notation (e.g. 'C5', 'F#5', 'Bb4') to MIDI note number."""
    if len(name) >= 2 and name[1] in ("#", "b"):
        pitch, octave = name[:2], int(name[2:])
    else:
        pitch, octave = name[:1], int(name[1:])
    midi_num = (octave + 1) * 12 + NOTE_OFFSETS[pitch]
    assert 0 <= midi_num <= 127, f"Invalid MIDI note: {name} -> {midi_num}"
    return midi_num


def encode_vlq(value: int) -> bytes:
    """Encode a non-negative integer as a MIDI Variable-Length Quantity."""
    if value < 0:
        raise ValueError(f"VLQ cannot be negative: {value}")
    chunks = [value & 0x7F]
    value >>= 7
    while value:
        chunks.append((value & 0x7F) | 0x80)
        value >>= 7
    chunks.reverse()
    return bytes(chunks)


def beats_to_ticks(beats: float) -> int:
    return max(0, int(round(beats * TPQN)))


@dataclass
class MidiTrackBuilder:
    name: str
    channel: int = 0
    program: int | None = None
    volume: int = 100
    pan: int = 64
    reverb: int = 40
    # Tuples of (tick, sort_priority, raw_bytes)
    events: List[Tuple[int, int, bytes]] = field(default_factory=list)

    def __post_init__(self) -> None:
        self.add_meta(0, 0x03, self.name.encode("utf-8"))
        if self.program is not None:
            # Bank Select MSB/LSB = 0, then Program Change
            self.add_cc(0.0, 0, 0, priority=1)
            self.add_cc(0.0, 32, 0, priority=1)
            self.events.append((0, 1, bytes([0xC0 | self.channel, self.program & 0x7F])))
            self.add_cc(0.0, 7, self.volume, priority=2)
            self.add_cc(0.0, 10, self.pan, priority=2)
            self.add_cc(0.0, 11, 127, priority=2)
            self.add_cc(0.0, 91, self.reverb, priority=2)

    def add_meta(self, tick: int, meta_type: int, data: bytes) -> None:
        payload = bytes([0xFF, meta_type]) + encode_vlq(len(data)) + data
        self.events.append((tick, 0, payload))

    def add_tempo(self, beat: float, bpm: float) -> None:
        us_per_quarter = int(round(60_000_000 / bpm))
        data = us_per_quarter.to_bytes(3, "big")
        self.add_meta(beats_to_ticks(beat), 0x51, data)

    def add_time_signature(self, beat: float, numerator: int = 4, denominator: int = 4) -> None:
        denom_pow = {1: 0, 2: 1, 4: 2, 8: 3, 16: 4}[denominator]
        self.add_meta(beats_to_ticks(beat), 0x58, bytes([numerator, denom_pow, 24, 8]))

    def add_cc(self, beat: float, controller: int, value: int, priority: int = 2) -> None:
        tick = beats_to_ticks(beat)
        v = max(0, min(127, int(round(value))))
        self.events.append((tick, priority, bytes([0xB0 | self.channel, controller & 0x7F, v])))

    def add_sustain(self, start_beat: float, end_beat: float) -> None:
        self.add_cc(start_beat, 64, 127)
        self.add_cc(end_beat, 64, 0)

    def add_expression_curve(
        self,
        start_beat: float,
        end_beat: float,
        start_val: int,
        end_val: int,
        steps: int = 12,
    ) -> None:
        for i in range(steps + 1):
            t = i / steps
            beat = start_beat + (end_beat - start_beat) * t
            val = start_val + (end_val - start_val) * t
            self.add_cc(beat, 11, int(round(val)))

    def add_note(self, pitch: str | int, start_beat: float, duration_beats: float, velocity: int) -> None:
        midi_num = note(pitch) if isinstance(pitch, str) else pitch
        vel = max(1, min(127, int(round(velocity))))
        start_tick = beats_to_ticks(start_beat)
        end_tick = beats_to_ticks(start_beat + duration_beats)
        if end_tick <= start_tick:
            end_tick = start_tick + 1
        self.events.append((start_tick, 4, bytes([0x90 | self.channel, midi_num, vel])))
        self.events.append((end_tick, 3, bytes([0x80 | self.channel, midi_num, 0])))

    def add_chord(
        self,
        pitches: List[str | int],
        start_beat: float,
        duration_beats: float,
        velocities: int | List[int],
        roll_beats: float = 0.0,
    ) -> None:
        for idx, p in enumerate(pitches):
            vel = velocities[idx] if isinstance(velocities, list) else velocities
            offset = idx * roll_beats
            self.add_note(p, start_beat + offset, max(0.05, duration_beats - offset), vel)

    def build_chunk(self) -> bytes:
        sorted_events = sorted(self.events, key=lambda e: (e[0], e[1]))
        body = bytearray()
        prev_tick = 0
        for tick, _, raw in sorted_events:
            delta = tick - prev_tick
            body.extend(encode_vlq(delta))
            body.extend(raw)
            prev_tick = tick
        # End of Track meta event
        body.extend(b"\x00\xFF\x2F\x00")
        return b"MTrk" + struct.pack(">I", len(body)) + bytes(body)


def write_midi_file(path: Path, bpm: float, tracks: List[MidiTrackBuilder]) -> None:
    tempo_track = MidiTrackBuilder(name="Tempo & TimeSig")
    tempo_track.add_time_signature(0.0, 4, 4)
    tempo_track.add_tempo(0.0, bpm)

    all_tracks = [tempo_track, *tracks]
    header = b"MThd" + struct.pack(">IHHH", 6, 1, len(all_tracks), TPQN)

    path.parent.mkdir(parents=True, exist_ok=True)
    with open(path, "wb") as f:
        f.write(header)
        for trk in all_tracks:
            f.write(trk.build_chunk())


def build_gentle_bell(midi_dir: Path) -> dict:
    """
    nalu-gentle-bell — Pomodoro Start ("轻柔铃音")
    Key: F major (Fmaj9), 108 BPM
    Role: Short, crystal-clear, low-intrusion focus cue (~2.9s).
    """
    bpm = 108.0

    # Lead celesta: delicate rising motif F5 -> G5 -> A5 -> shimmering C6 bloom
    celesta = MidiTrackBuilder(
        name="Celesta Lead",
        channel=0,
        program=GM_CELESTA,
        volume=114,
        pan=54,
        reverb=46,
    )
    celesta.add_sustain(0.0, 3.6)
    celesta.add_note("C5", 0.00, 1.4, 62)
    celesta.add_note("F5", 0.03, 1.5, 80)
    celesta.add_note("G5", 0.55, 1.3, 74)
    celesta.add_note("A5", 1.08, 2.0, 88)
    celesta.add_note("C6", 1.13, 1.9, 60)

    # Warm vibraphone cushion underneath
    vibes = MidiTrackBuilder(
        name="Vibraphone Harmony",
        channel=1,
        program=GM_VIBRAPHONE,
        volume=88,
        pan=74,
        reverb=52,
    )
    vibes.add_sustain(0.0, 3.6)
    vibes.add_chord(["F4", "A4"], 0.00, 1.6, [58, 54], roll_beats=0.04)
    vibes.add_chord(["C5", "E5"], 1.08, 2.0, [62, 56], roll_beats=0.03)

    # Airy harp grace pluck at entry
    harp = MidiTrackBuilder(
        name="Harp Shimmer",
        channel=2,
        program=GM_ORCHESTRAL_HARP,
        volume=76,
        pan=64,
        reverb=48,
    )
    harp.add_note("F4", 0.00, 1.2, 52)
    harp.add_note("C5", 0.16, 1.2, 56)
    harp.add_note("A5", 0.34, 1.3, 48)

    out_path = midi_dir / "nalu-gentle-bell.mid"
    write_midi_file(out_path, bpm, [celesta, vibes, harp])
    return {
        "id": "gentle-bell",
        "midi": "scripts/sounds/midi/nalu-gentle-bell.mid",
        "wav": "static/sounds/nalu-gentle-bell.wav",
        "duration": 2.9,
        "reverb": 26.0,
        "fadeOut": 0.30,
        "peakDb": -2.0,
    }


def build_warm_chime(midi_dir: Path) -> dict:
    """
    nalu-warm-chime — Default Alarm ("暖色和弦")
    Key: D major (Dmaj9 -> Gmaj7 -> Dmaj9), 120 BPM
    Role: Recognizable, warm, never harsh; loops seamlessly at exact 4.000s (2 bars of 4/4).
    """
    bpm = 120.0

    # Lead vibraphone: clear two-phrase acoustic alarm melody
    vibes = MidiTrackBuilder(
        name="Vibraphone Chime",
        channel=0,
        program=GM_VIBRAPHONE,
        volume=114,
        pan=52,
        reverb=42,
    )
    vibes.add_sustain(0.0, 2.15)
    vibes.add_sustain(2.22, 4.15)
    vibes.add_sustain(4.22, 7.90)
    # Phrase 1 (Bar 1, beats 0.0 - 2.0): uplifting ascending chime
    vibes.add_note("F#5", 0.00, 0.75, 84)
    vibes.add_note("A5",  0.48, 0.75, 80)
    vibes.add_note("D6",  0.98, 1.15, 92)
    # Phrase 2 (Bar 1.5 - Bar 2, beats 2.25 - 7.5): warm resolving answer
    vibes.add_note("B5",  2.25, 0.65, 80)
    vibes.add_note("A5",  2.75, 0.65, 76)
    vibes.add_note("F#5", 3.25, 0.65, 78)
    vibes.add_note("E5",  3.75, 0.60, 72)
    vibes.add_note("F#5", 4.25, 3.30, 86)

    # Warm Rhodes electric piano chord bed
    ep = MidiTrackBuilder(
        name="Electric Piano Bed",
        channel=1,
        program=GM_ELECTRIC_PIANO_1,
        volume=100,
        pan=76,
        reverb=44,
    )
    ep.add_sustain(0.0, 2.15)
    ep.add_sustain(2.22, 4.15)
    ep.add_sustain(4.22, 7.90)
    ep.add_chord(["D4", "A4", "E5", "F#5"], 0.00, 2.05, [66, 68, 70, 72], roll_beats=0.03)
    ep.add_chord(["G4", "B4", "D5", "F#5"], 2.25, 1.85, [64, 66, 68, 70], roll_beats=0.03)
    ep.add_chord(["D4", "A4", "C#5", "E5", "F#5"], 4.25, 3.30, [64, 68, 64, 66, 70], roll_beats=0.03)

    # Marimba subtle rhythmic accent for unmistakable alarm presence
    marimba = MidiTrackBuilder(
        name="Marimba Pulse",
        channel=2,
        program=GM_MARIMBA,
        volume=92,
        pan=64,
        reverb=30,
    )
    marimba.add_note("D5", 0.00, 0.45, 76)
    marimba.add_note("A5", 0.98, 0.45, 70)
    marimba.add_note("G5", 2.25, 0.45, 72)
    marimba.add_note("D5", 4.25, 0.85, 76)

    # Subtle celesta bell gleam on the two peaks
    celesta = MidiTrackBuilder(
        name="Celesta Sparkle",
        channel=3,
        program=GM_CELESTA,
        volume=76,
        pan=68,
        reverb=50,
    )
    celesta.add_sustain(0.95, 7.90)
    celesta.add_note("D6", 1.00, 1.30, 62)
    celesta.add_note("A5", 4.28, 3.00, 58)
    celesta.add_note("D6", 4.33, 3.00, 52)

    out_path = midi_dir / "nalu-warm-chime.mid"
    write_midi_file(out_path, bpm, [vibes, ep, marimba, celesta])
    return {
        "id": "warm-chime",
        "midi": "scripts/sounds/midi/nalu-warm-chime.mid",
        "wav": "static/sounds/nalu-warm-chime.wav",
        "duration": 4.0,  # Exact 2 bars @ 120 BPM for seamless looping
        "reverb": 22.0,
        "fadeOut": 0.45,
        "peakDb": -1.5,
    }


def build_soft_rise(midi_dir: Path) -> dict:
    """
    nalu-soft-rise — Pomodoro End ("渐进晨光")
    Key: Eb major (Abmaj7 -> Bbadd9 -> Ebmaj9), 96 BPM
    Role: Blooming sunrise arpeggio with clear, gratifying completion resolve (~4.0s).
    """
    bpm = 96.0

    # Rising orchestral harp arpeggio (crescendo)
    harp = MidiTrackBuilder(
        name="Harp Sunrise Arpeggio",
        channel=0,
        program=GM_ORCHESTRAL_HARP,
        volume=106,
        pan=54,
        reverb=52,
    )
    harp.add_sustain(0.0, 6.2)
    arpeggio = [
        ("Eb4", 0.00, 50),
        ("G4",  0.26, 58),
        ("Bb4", 0.52, 65),
        ("D5",  0.78, 72),
        ("F5",  1.04, 79),
        ("G5",  1.30, 84),
        ("Bb5", 1.56, 88),
    ]
    for pitch, beat, vel in arpeggio:
        harp.add_note(pitch, beat, 2.8, vel)

    # Radiant celesta melody crest & resolution
    celesta = MidiTrackBuilder(
        name="Celesta Resolution",
        channel=1,
        program=GM_CELESTA,
        volume=110,
        pan=72,
        reverb=54,
    )
    celesta.add_sustain(1.4, 6.2)
    celesta.add_note("Bb5", 1.56, 1.4, 74)
    celesta.add_note("D6",  1.90, 1.5, 84)
    celesta.add_note("Eb6", 2.26, 3.6, 94)
    celesta.add_note("G5",  2.29, 3.5, 68)

    # Warm Rhodes + Bell EP harmonic foundation (Abmaj7 -> Bbadd9 -> Ebmaj9)
    ep = MidiTrackBuilder(
        name="Rhodes Sunrise Harmony",
        channel=2,
        program=GM_ELECTRIC_PIANO_1,
        volume=94,
        pan=46,
        reverb=48,
    )
    ep.add_sustain(0.0, 0.98)
    ep.add_sustain(1.04, 2.18)
    ep.add_sustain(2.24, 6.2)
    ep.add_chord(["Ab3", "Eb4", "G4"], 0.00, 0.98, [52, 56, 54], roll_beats=0.04)
    ep.add_chord(["Bb3", "F4", "C5"], 1.04, 1.12, [60, 64, 62], roll_beats=0.04)
    ep.add_chord(["Eb4", "G4", "Bb4", "D5", "F5"], 2.26, 3.6, [68, 72, 74, 68, 64], roll_beats=0.03)

    # Shimmering vibraphone bloom on the final tonic chord
    vibes = MidiTrackBuilder(
        name="Vibraphone Sunrise Bloom",
        channel=3,
        program=GM_VIBRAPHONE,
        volume=86,
        pan=66,
        reverb=56,
    )
    vibes.add_sustain(1.0, 6.2)
    vibes.add_chord(["F4", "Bb4", "D5"], 1.04, 1.2, [46, 50, 52], roll_beats=0.05)
    vibes.add_chord(["G4", "Bb4", "Eb5", "G5"], 2.26, 3.6, [62, 66, 70, 64], roll_beats=0.04)

    out_path = midi_dir / "nalu-soft-rise.mid"
    write_midi_file(out_path, bpm, [harp, celesta, ep, vibes])
    return {
        "id": "soft-rise",
        "midi": "scripts/sounds/midi/nalu-soft-rise.mid",
        "wav": "static/sounds/nalu-soft-rise.wav",
        "duration": 4.0,
        "reverb": 28.0,
        "fadeOut": 0.55,
        "peakDb": -1.8,
    }


def main() -> None:
    repo_root = Path(__file__).resolve().parents[2]
    midi_dir = repo_root / "scripts" / "sounds" / "midi"
    midi_dir.mkdir(parents=True, exist_ok=True)

    manifest = [
        build_gentle_bell(midi_dir),
        build_warm_chime(midi_dir),
        build_soft_rise(midi_dir),
    ]

    manifest_path = midi_dir / "manifest.json"
    with open(manifest_path, "w", encoding="utf-8") as f:
        json.dump(manifest, f, indent=2, ensure_ascii=False)
        f.write("\n")

    for item in manifest:
        print(f"[generate-midi] wrote {item['midi']} ({item['duration']}s target)")


if __name__ == "__main__":
    main()
