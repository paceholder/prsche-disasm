# Building the project

## Active C/C++ recovery

Requirements: Windows x64, Visual Studio 2022 Build Tools with the x86/x64 C++ workload,
a Windows SDK, CMake and Python 3. [scripts/tool-env.ps1](../scripts/tool-env.ps1) locates
installed tools and project-local tool directories. The compiler target is Win32/x86.

From the repository root:

```powershell
./scripts/build-v2.ps1
```

This builds recovered modules and verification programs under
`local/builds/v2/001-original-recovery`; `recovery_probe.exe` is a verification tool,
not the game. Original binaries/resources belong in read-only `local/game/`.
[game-build.md](game-build.md) documents the reference installation.

The latest full game-link build is documented by [STATUS.md](../STATUS.md) and its
linked run README. That run's build-config drives a fresh configure/build and a
shared acceptance audit. The game currently has unresolved providers; a successful
module build does not imply a playable game executable. Generated builds and proof
caches are local artifacts, not files to commit.

## Rust prototype instructions

The instructions below describe the unfinished, paused 012 prototype. They do not
build the active C++ game. Snapshots 001?011 remain preserved.

# Сборка активной итерации

Из корня репозитория выполните одну команду:

```powershell
./scripts/build.ps1
```

По умолчанию собирается активная `012-campaign-fidelity` в конфигурации `release`.
Windows executable: `local/builds/012-campaign-fidelity/windows/porsche-viewer.exe`.
Web-пакет: `local/builds/012-campaign-fidelity/web`, статический UI плюс wasm-bindgen
в `package` (`viewer_impl.js`, `viewer_impl_bg.wasm` и сопутствующий `.d.ts`).

Для оригинальных картинок интерфейса один раз выполните `./scripts/extract-fe-ui.ps1`.
Скрипт читает `local/game/FEData` и создаёт 119 PNG в `local/derived/fe-ui`;
локальный сервер активной 012 отдаёт их по `/assets/`. Кэш и игровые ресурсы не входят
в Git или web-пакет. Повторяющиеся ID внутри FSH сохраняют прежнее правило последнего
элемента, выбор записывается в выводе извлечения. Это ещё не восстановление оригинальных
шрифтов, комнат и всех слоёв интерфейса.

Точные результаты и незакрытые проверки активной итерации описаны в корневых
[PLAN.md](../PLAN.md), [STATUS.md](../STATUS.md) и её [checkpoint run](../iterations/012-campaign-fidelity/runs/001-fidelity-repair/README.md).
Снимки 001–011 заморожены; отчёты и проверки прошлых итераций не доказывают
полного соответствия оригиналу 1:1.

Запуск: `./scripts/launch-viewer.ps1 -Mode desktop|web|native` (выбрать одно значение).
Дополнительный desktop использует тот же web-пакет в отдельном окне Edge/Chrome;
нужны установленный браузер и Python для локального read-only сервера.
Helper слушает свободный loopback-порт, PID проверяется, логи каждого запуска
сохраняются в `local/builds/<iteration>/launcher/<id>`. Остановка сервера — Ctrl+C
в консоли launcher; закрытие окна браузера пока не останавливает сервер автоматически.
Native работает самостоятельно: без аргументов — каталог; `catalog --game-dir <path>`
выбирает другой источник; прежние `inspect/view` сохранены.

Параметры `-Iteration`, `-Config debug|release` и `-Target all|native|web` позволяют выбрать снимок, конфигурацию или часть сборки. Скрипт требует Rust `1.98.1` из `rust-toolchain.toml`, проверенный `Cargo.lock` в корне итерации и `wasm-bindgen-cli 0.2.128` на `PATH`; для нового снимка сначала создайте lock-файл командой `cargo generate-lockfile --manifest-path iterations/<iteration>/Cargo.toml`. Окружение можно подготовить через `./scripts/tool-env.ps1`.

Игровые файлы не копируются в результаты сборки. Все итерации используют общий `local/game`; путь можно явно указать параметром `-GameDir`, а при запуске передать viewer его штатный `--game-dir <path>`.
