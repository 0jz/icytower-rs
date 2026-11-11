# Icy Tower rebuild (Rust + macroquad)

## Project map
- `src/game.rs` — all gameplay logic, exact port of the original exe (player.c, map.c, main.c `play()`, collision_type 2, msvcrt rand for floors); pure, unit-tested.
- `src/f80.rs` — x87 80-bit soft-float; needed so replays match the original bit for bit.
- `src/main.rs` — asset loading, in-game renderer (layout from original `draw_frame`), letterbox camera, main loop.
- `src/app.rs` — all screens: title (main_menu_callback layout), menus, profile view/select/create, replay list/save/watch (VCR), post-game replay_menu, alerts, 50Hz fixed tick.
- `src/menu.rs` — menu tree (labels from the exe's menu tables), settings + top-10 saved to `icytower.cfg` next to the exe.
- `src/profile.rs` — profiles in `<exe dir>/profiles/<name>/` (profile.cfg + original-format `<name>_stats.txt`), ranks, replays in the original binary `.itr` (ITR140) format incl. checksum; original replays play back exactly (test `original_replays_play_back_exactly`). First run imports `../profiles/*/<name>_stats.txt` from the original game.
- Camera: `Camera2D::from_display_rect` is y-up in this macroquad, so main.rs negates `zoom.y`.
- `tools/extract.py` — decodes the original Allegro 4 datafiles into `assets/` (PNG sprites, OGG sounds, font strips).
- `assets/` — generated; `data/` (data.dat), `sfx15/`, one folder per character (harold, dave, jane, wendy).
- The original game lives one level up (`../icytower15.exe`, `../data`, `../characters`).
- HD (plan B): `tools/upscale.py` builds `assets_hd/` (Scale2x twice = 4x; git-ignored). main.rs `Tex` holds sd + hd textures and always reports the original size; its `draw_texture(_ex)` shadow macroquad's, so draw code is unchanged. GFX Options → HD Graphics.

## Commands
- Extract assets: `python -I tools/extract.py .. assets`
- Build HD assets: `python -I tools/upscale.py assets assets_hd`
- Test: `cargo test --offline`
- Run: `cargo run --release --offline`

## Reverse-engineering notes
- `icytower15.exe` ships full DWARF debug info: `gdb -batch -ex "ptype Tplayer" ../icytower15.exe`,
  `objdump -d -l -M intel --start-address=... ../icytower15.exe` gives source line numbers.
- data.dat / sfx15.dat: XOR key `CHEESE` from byte 4 (index `(pos) % 6`), then standard Allegro packfile/LZSS.
  loading.dat uses another key (skipped, not needed).
- Original runs logic at 50Hz (`install_int(cycle_counter, 20)`), 640x480, `new_game` sets collision_type 2 = `handle_player_collision_vector`.
- Character frames (datafile order): 1,10,11 idle; 2-5 walk; 6-8 jump up/mid/down; 9 straight jump; 12 chock; 13 spin; 14-15 edge.
