#!/usr/bin/env node
/**
 * Build Nalu's built-in alert WAV sounds from MIDI sources.
 *
 * Usage:
 *   node scripts/sounds/build-sounds.mjs                # regenerate .mid + render .wav
 *   node scripts/sounds/build-sounds.mjs --render-only  # render existing .mid -> .wav (for DAW-edited MIDI)
 *   node scripts/sounds/build-sounds.mjs --soundfont /path/to/custom.sf2
 */

import { execFileSync } from "node:child_process";
import path from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = path.resolve(__dirname, "../..");
const args = process.argv.slice(2);

const renderOnly = args.includes("--render-only");
const soundfontIdx = args.indexOf("--soundfont");
const soundfontPath = soundfontIdx >= 0 ? args[soundfontIdx + 1] : null;

function run(cmd, cmdArgs) {
  execFileSync(cmd, cmdArgs, { cwd: repoRoot, stdio: "inherit" });
}

if (!renderOnly) {
  run("python3", [path.join(__dirname, "generate-midi.py")]);
}

const swiftArgs = [
  "-suppress-warnings",
  path.join(__dirname, "render-midi.swift"),
  "--manifest",
  path.join(__dirname, "midi/manifest.json"),
  "--root",
  repoRoot,
];

if (soundfontPath) {
  swiftArgs.push("--soundfont", soundfontPath);
}

run("swift", swiftArgs);
