# Drittanbieter-Software

Der Rust-Quellcode dieses Projekts steht unter der MIT-Lizenz in `LICENSE`.

## FFmpeg und ffprobe

Die Windows-x64-Anwendung enthält die unveränderten Programme `ffmpeg.exe` und `ffprobe.exe` aus FFmpeg 9.0.2 Essentials von Gyan.dev. Sie werden als separate Prozesse ausgeführt. Der nicht benötigte Player ffplay wird nicht eingebettet.

- Herausgeber und Buildinformationen: https://www.gyan.dev/ffmpeg/builds/
- Ursprüngliches Archiv: https://www.gyan.dev/ffmpeg/builds/packages/ffmpeg-9.0.2-essentials_build.zip
- Archiv-SHA-256: `60f467265b1e312373dbcd92200c2618a74850f98d3d078e94296bb3fa2047ba`
- Lizenz: GNU GPL Version 3; der vollständige Text liegt jedem Release als `FFmpeg-LICENSE.txt` bei und ist in der App eingebettet.
- Die ursprüngliche `FFmpeg-README.txt` enthält die genaue Quellcode-Revision, Buildkonfiguration und Informationen zu den eingebundenen Bibliotheken.
- FFmpeg-Quellcode: https://github.com/FFmpeg/FFmpeg

Die MIT-Lizenz dieses Projekts ersetzt keine Lizenz der enthaltenen Drittanbieter-Programme oder Rust-Bibliotheken.

## Inspiration

Das eigenständige Rust-Projekt ist inspiriert von https://github.com/VOiD-Ayush/steam-showcase-maker. Dessen Go- und Frontend-Quellcode wurde nicht übernommen.
