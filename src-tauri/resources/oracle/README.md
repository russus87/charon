# Oracle Instant Client impacchettato (bundle.resources)

Questa cartella viene inclusa in ogni artefatto (`.exe`/`.msi`, `.AppImage`,
`.pkg.tar.zst`, `.deb`/`.rpm`) tramite `tauri.conf.json → bundle.resources`.

In **CI** (`.github/workflows/release.yml`) uno step scarica il **Basic Lite**
giusto per l'OS in `instantclient-basiclite-<os>.x64-<ver>.zip` e lo mette qui,
prima di `tauri build`. A runtime Charon lo trova nella resource dir, lo
scompatta (ricreando i symlink) e lo aggancia — zero setup per l'utente.

I binari Oracle **non sono committati** (vedi `.gitignore`): sono di proprietà
Oracle e pesanti. In locale puoi metterci lo zip a mano per provare il bundle.

macOS: l'Instant Client arm64 è distribuito come `.dmg` (non zip) → per ora su
macOS si usa `oracle-setup`.
