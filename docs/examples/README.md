# Beispiel: Katze im Burghof

Die Vorlage wurde vom Nutzer für das Import-/Export-Beispiel und die README-Screenshots bereitgestellt. `katze-burghof.png` ist eine unveränderte Kopie der Datei „Katze ärgert Nachtelfen-Schurken im Burghof.png“.

## Gezeigter Export

- Layout: vier Streifen nebeneinander (`strip4`)
- Format: PNG (Standbild)
- Panelbreite: 150 Pixel
- Panelhöhe: 338 Pixel
- Abstand im Motiv: 0 Pixel
- Startzeit: 0 Sekunden
- Effekte: deaktiviert

Die vier Dateien in `export/` wurden mit der Desktop-App erzeugt. Der zweite Screenshot zeigt separat die Vorschau mit Schwarzweiß-Effekt; dieser Effekt ist auf die hier abgelegten farbigen Exportdateien nicht angewendet.

Zum Nachvollziehen per Kommandozeile:

```powershell
cargo run --features console -- export docs/examples/katze-burghof.png --layout strip4 --format png --width 150 --height 338 --output exports
```

`export/manifest.json` enthält die Einstellungen und gemessenen Größen. Das Beispielbild ist kein Test einer animierten GIF-Quelle und zeigt keinen Steam-Upload.
