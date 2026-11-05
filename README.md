# beatd

[![CI](https://github.com/nuskey8/beatd/actions/workflows/ci.yml/badge.svg)](https://github.com/nuskey8/beatd/avctions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/nuskey8/beatd)](https://github.com/nuskey8/beatd/releases/latest)
[![crates.io](https://img.shields.io/crates/v/beatd)](https://crates.io/crates/beatd)
[![npm](https://img.shields.io/npm/v/beatd-js)](https://www.npmjs.com/package/beatd-js)
[![NuGet](https://img.shields.io/nuget/v/Beatd)](https://www.nuget.org/packages/Beatd)
[![Unity](https://img.shields.io/badge/Unity-6000.0%2B-black?logo=unity)](https://github.com/nuskey8/beatd/tree/main/beatd-dotnet/Beatd.Unity/Packages/com.nuskey8.beatd.unity)

English | [日本語](https://github.com/nuskey8/beatd/blob/main/README.ja.md)

A cross-platform GUI application and library for analyzing streaming audio and converting it to OSC.

![hero](https://raw.githubusercontent.com/nuskey8/beatd/main/assets/hero.png)

## Overview

beatd is a cross-platform application and library for streaming audio analysis.

It started with the concept of being a modern reimplementation of [Sound2Light](https://github.com/ETCLabs/Sound2Light), utilizing a Rust implementation to create an easy-to-use, cross-platform GUI. It can send audio analysis results via OSC to integrate with external VJ software and similar tools.

Additionally, the core analysis engine is provided as an independent library so that it can be embedded into applications for Rust, C, C# (.NET/Unity), and JavaScript (WebAssembly).

## Screenshot

![img](https://raw.githubusercontent.com/nuskey8/beatd/main/assets/img1.png)

## Installation

Select the latest release from GitHub [Releases](https://github.com/nuskey8/beatd/releases/latest) and download the executable file for your operating system.

### macOS

The macOS build is not signed or notarized with an Apple Developer ID. On first launch, macOS may show a message such as "beatd cannot be opened because the developer cannot be verified."

To open it anyway:

1. Right-click (or Control-click) `beatd.app`, choose **Open**, then click **Open** again.
2. Or allow it from **System Settings → Privacy & Security** after trying to open it once.
3. Or remove the quarantine attribute in Terminal:

```sh
xattr -dr com.apple.quarantine /Applications/beatd.app
```

## libbeatd

The streaming audio analysis in beatd is implemented in Rust as an independent crate. By embedding this, you can use features such as BPM analysis within your applications.

### Rust

Add the crate via cargo.

```sh
cargo add beatd
```

```rust
use beatd::Analyzer;

let mut analyzer = Analyzer::new(48_000);

// samples: &[f32] (-1.0 to 1.0)
analyzer.push(&samples);

let level = analyzer.level();
let bpm = analyzer.bpm();
let spectrum = analyzer.spectrum();
let waveform = analyzer.waveform();
let beats = analyzer.beat_offsets(bpm);
```

### C

Build the library with the `ffi` feature enabled.

```sh
cargo build -p beatd --features ffi
```

Add the generated `libbeatd.so` / `libbeatd.dylib` / `beatd.dll` along with `beatd/beatd.h` to your project, and specify `-lbeatd` during linking.

```c
#include "beatd.h"

beatd_analyzer *analyzer = beatd_new(48000);
if (analyzer == NULL) {
    return 1;
}

beatd_set_bpm_range_min(analyzer, 75);
beatd_push(analyzer, samples, count);

float level = beatd_level(analyzer);
float bpm = beatd_bpm(analyzer);

float spectrum[1024];
size_t bins = beatd_spectrum(analyzer, spectrum, 1024);

beatd_free(analyzer);
```

### JavaScript

Add the library via npm.

```sh
npm i beatd-js
```

```js
import { createAnalyzer } from "beatd-js";

using analyzer = createAnalyzer({ sampleRate: 48000 });

// samples: Float32Array
analyzer.push(samples);

console.log(analyzer.level(), analyzer.bpm());
console.log(analyzer.spectrum());
console.log(analyzer.waveform());
console.log(analyzer.beatOffsets(analyzer.bpm()));
```

### C# / Unity

For .NET projects, you can add the package from NuGet.

```sh
dotnet package add Beatd
```

For Unity, you can add the package from the Package Manager using the git URL.

```
https://github.com/nuskey8/beatd.git?path=beatd-dotnet/Beatd.Unity/Packages/com.nuskey8.beatd.unity
```

```csharp
using Beatd;

using var analyzer = new BeatdAnalyzer(48_000);
analyzer.BpmRangeMin = 75;

analyzer.Push(samples); // ReadOnlySpan<float>

Console.WriteLine($"{analyzer.Level}, {analyzer.Bpm}");
float[] spectrum = analyzer.GetSpectrum();
float[] beats = analyzer.GetBeatOffsets(analyzer.Bpm);
```

Please note that the .NET/Unity version currently supports only the following platforms:

| Platform            | RID           | .NET (NuGet) | Unity |
| :------------------ | :------------ | :----------: | :---: |
| Windows x64         | `win-x64`     |      ✅      |  ✅   |
| Windows ARM64       | `win-arm64`   |      ✅      |  ✅   |
| Linux x64           | `linux-x64`   |      ✅      |  ✅   |
| Linux ARM64         | `linux-arm64` |      ✅      |  ✅   |
| macOS Intel         | `osx-x64`     |      ✅      |  ✅   |
| macOS Apple Silicon | `osx-arm64`   |      ✅      |  ✅   |

## License

beatd is released under the [MIT License](https://github.com/nuskey8/beatd/blob/main/LICENSE).
