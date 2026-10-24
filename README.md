# Need for Speed: Porsche Unleashed — восстановление оригинала

Цель проекта — восстановить игру из оригинальных бинарников в C/C++, собрать
рабочую Windows/x86 версию, затем описать устройство кода и перенести его на Rust
для современных native и web платформ. В объём входят обе карьеры, физика,
повреждения, ИИ, экономика, сохранения, интерфейс и звук.

Алгоритмы восстанавливаются по дизассемблеру и исходным ресурсам. Поведение
проверяется сравнением входов, состояния и выходов с оригиналом.
Правила работы: [AGENTS.md](AGENTS.md).

## Текущий результат

Current2026-10-24: [Run398](iterations/v2/001-original-recovery/runs/398-network-member-integration/README.md)
compiles216TUs;196 unresolved names/204refs,no gameEXE.
[PR24/Run431](iterations/v2/001-original-recovery/runs/431-x86-backend-exact/README.md)
adds reviewed research lowering, outside the game graph. Older checkpoints below
are historical; active work is tracked in the iteration PLAN.

[PR20 research tooling](docs/research/pr20-acceptance-2026-10-20.md) adds an
experimental x86 lowering/oracle tool after review corrections.107 supported
fragments/109568 native cases pass; unsupported x87 boundaries are explicit.
It is outside the production game graph and does not replace original recovery.

[Очередь PR #2–#19 разобрана](docs/research/pr-triage-2026-10-19.md): #3 слит,
остальные17 закрыты с объяснениями; открытых PR нет. Непринятый полезный код
сохранён как кандидаты в очереди восстановления. Продолжаем основной путь с REA.

Активна [v2 / 001 — восстановление исходного кода](iterations/v2/001-original-recovery/README.md).
**Полная игра из восстановленных исходников пока не запускается.**

На 2026-10-19:

- [Compile candidates, Run274](iterations/v2/001-original-recovery/runs/274-shared-heap-union/README.md):
  1125 разных адресов функций скомпилированы в x86 COFF и объединены в библиотеку.
  Принудительная линковка всего архива оставляет 414 неразрешённых имён; EXE/DLL не получен.
  Успешная компиляция этих кандидатов ещё не подтверждает точность поведения.
- [Проверенная интеграция, Run173](iterations/v2/001-original-recovery/runs/173-float-transport-integration/README.md):
  476 полностью восстановленных функций и 6 частичных consumers;
  24 560 сравнений с оригиналом и 12 native/link проверок прошли.
  Это отдельный проверенный набор, а не ещё 482 функции поверх 1125 кандидатов.

[Статический перевод x86→C++, Run258](iterations/v2/001-original-recovery/runs/258-x86-call-chain/README.md):
три оригинальные цепочки CALL/RET, 36 864 сравнения регистров, флагов, памяти и стека
прошли. Это проверка экспериментального backend, а не запуск полной игры.

[Latest game build, Run388](iterations/v2/001-original-recovery/runs/388-startup-integration/README.md):
211 C++ TUs compile;194 names/201 refs,noEXE. Startupcallback/heap handoff passed
1504 native comparisons/15 mutations;528-input audit PASS. Next: newPRreview.
Full iteration/startup/menu criteria remain open.

Общий layout и инициализация корневого объекта остаются открытыми. Срок полной игры
не установлен.

[REA CLI и постоянная MCP-сессия](iterations/v2/001-original-recovery/runs/316-rea-fill-session/README.md)
проверены на Windows/x86 без отдельного API: scalar/MMX/SSE в одном соединении.
Холодный анализ91.38с; повтор39мс, соседние функции73/138мс. Результаты анализа
проверяются по оригинальному коду; сама декомпиляция не доказывает точность C++.
Точная граница и результаты: [STATUS.md](STATUS.md).

## Сборка текущих C++ целей

Нужны Windows x64, Visual Studio 2022 Build Tools с C++ x86/x64 workload,
Windows SDK и CMake. Для исследовательских скриптов также нужен Python 3.
Настройка окружения: [docs/building.md](docs/building.md) и
[scripts/tool-env.ps1](scripts/tool-env.ps1).

Из корня репозитория:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-v2.ps1
```

Скрипт собирает существующие CMake цели MSVC Win32 в конфигурации Release.
Результаты: `local/builds/v2/001-original-recovery/`.
Это сборка восстановленных модулей и проверочных программ; она пока не создаёт
полностью рабочую игру. Экспериментальный корпус кандидатов собирается отдельно;
команда последнего объединения приведена в [Run274](iterations/v2/001-original-recovery/runs/274-shared-heap-union/README.md).

Оригинальные бинарники и ресурсы располагаются в `local/game/` и используются
только для чтения. Локальные эксперименты и сборки хранятся вне Git.

## Навигация

- [План проекта](PLAN.md) и [план активной итерации](iterations/v2/001-original-recovery/PLAN.md).
- [Индекс бинарников](research/binary-index/README.md) — первая точка поиска оригинального кода.
- [Дополнительные диапазоны](research/binary-index/supplemental/README.md) — навигация по пропускам, с явным статусом доказательств.
- [Отложенные проверки](docs/recovery-validation-backlog.md) — обязательная очередь до итоговой приёмки.
- [Общие манифесты входов](research/v2/proof-manifests/README.md) — повторяющиеся SHA хранятся один раз, результаты прогонов отдельно.
- [Автоматизация восстановления](docs/recovery-automation.md) — сборка, обратная связь и очередь зависимостей.
- [Сохранённая незавершённая работа](docs/recovery-wip.md).
- [Документация](docs/README.md) и [правила исполнителей](docs/agents.md).

## Предыдущий прототип v1

Rust/WebGPU прототипы сохранены в `iterations/`. Снимки 001–011 заморожены.
[012 — campaign fidelity](iterations/012-campaign-fidelity/README.md) осталась
незавершённой; её интеграция остановлена после перехода на восстановление v2.
Инструкции запуска и управление прототипом находятся в README этой итерации.

## License

Project-owned original code and documentation use the [MIT License](LICENSE),
subject to the [licensing scope](LICENSING.md). Original game material and
third-party contributions are not automatically relicensed by this notice.

## Acknowledgements

При исследовании CRP и FSH использовались открытые описания и реализации:
[CrpLib / FSHTool, Denis Auroux](https://auroux.free.fr/),
[OpenNFS / LibOpenNFS](https://github.com/OpenNFS/LibOpenNFS) и
[dima424658 / LibOpenNFS](https://github.com/dima424658/LibOpenNFS).
