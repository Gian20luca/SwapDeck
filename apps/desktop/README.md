# App desktop

`src-tauri` contiene per ora un segnaposto che compila il binario `swapdeck` e usa `swapdeck-core`.
L'app Tauri 2 (area di notifica, finestre, `--headless`) arriva con lo step 4 del piano.

## Build locale su Windows senza Build Tools MSVC

- Toolchain rustup `stable-x86_64-pc-windows-gnullvm` come predefinito (`rustup default ...`).
- [llvm-mingw](https://github.com/mstorsjo/llvm-mingw), archivio `ucrt-x86_64` con digest verificato,
  estratto in `%LOCALAPPDATA%\Programs` e con la cartella `bin` nel PATH utente.
- Non usare il toolchain `x86_64-pc-windows-gnu` insieme a llvm-mingw: il linker cerca `libgcc` e fallisce.

Verificato il 2026-10-15: un'app Tauri 2 minima compila (circa 10 minuti la prima volta).
L'eseguibile dipende da `libunwind.dll` di llvm-mingw e da `WebView2Loader.dll`, quindi serve solo
allo sviluppo; gli installer escono dalla CI con MSVC.
