# 012-campaign-fidelity

## Disclaimer and game data

- This project is not affiliated with, endorsed by or sponsored by Electronic Arts or
  Porsche. *Need for Speed*, *Porsche Unleashed* and Porsche names and marks are trademarks
  of their respective owners.
- The repository contains no game assets: no models, textures, sounds, tracks, executables,
  car data or recordings made from them. Screenshots are limited to a few illustrations.
- Running the engine requires your own legitimate copy of the original game; the engine reads
  the files from it at run time.
- Tests that compare the port with the original executable use data derived from the game.
  That data is not committed: `scripts/research/record-local-fixtures.sh` regenerates it from
  your own game into the gitignored `local/fixtures/` (`--check` compares it with the recorded
  SHA-256 sums in `scripts/research/local-fixtures.sha256`). Without it these tests skip and
  print a `SKIPPED` line. See [LOCAL-FIXTURES.md](LOCAL-FIXTURES.md) for the steps and a
  list of what is recorded.

2026-10-06, [Run 019](runs/019-original-contact-runtime/README.md): EDG и
type-1 опоры в одном игровом дереве, 15 трасс проверены. Первая body selection,
подготовка/response и wheel material state перенесены в исходные API и сверены
с x86 (3 дерева / 27 queries / 8 body / 45 responses / 5 height / 29 material).
220 тестов, fmt/clippy/wasm32, native/web, 3 Quick Race и 21 browser check прошли.
Силы/body response в игре пока adapter; поля, cadence и полный force loop не
завершены. UI/миссия/визуальная сверка обязательны. 012 открыта; активных
исполнителей нет, координатор завершил и проверил частичный код после лимита.

2026-10-06, [Run 018](runs/018-original-support-runtime/README.md): type-1
опоры подключены к игровым колёсам и surface queries. Реальные CRP проходят
ordinary/retained flag-1 consumer, mt/vt/df/pr сборку, дерево и исходные кэши/
плоскость; RD*/Skidpad material-name отбор больше не используется.
15 трасс / 237 395 полигонов; x86: 9 loader + 10 owner cases. 215 тестов,
fmt/clippy/wasm32 и native/web release прошли. Quick Race на трёх трассах и
21 общий browser check прошли. Сборка — local/builds/012-campaign-fidelity.
Original force solver, complete impact path и mixed-object tree не завершены;
это не доказательство совпадения езды/визуала с оригиналом. 012 открыта.

2026-10-05, [Run 017](runs/017-original-support-insertion/README.md): исходные
вставка/деление дерева соединены с полным cached support query. 36 деревьев и
72 запроса совпали с x86, включая record order/cache; 38+304 callback/predicate
cases проверены отдельно. 20 тестов, fmt/clippy/wasm32 прошли. Далее полный
ресурсный loader и state/cadence; первая игровая трасса — цель внутри 012.
Новая связка пока вне заезда, build fa56d05. 012 открыта.

2026-10-05, [Run 016](runs/016-original-support-assembly/README.md): перенесены
исходные Base/pr readers и child constructor дерева. 317 primitive / 60 Base /
256 child случаев сверены с исходным x86; Rust проверил 74 300 pr на 15 трассах.
16 тестов, fmt/clippy/wasm32 прошли. Далее рекурсивная вставка и полная сборка
опор; подключение к заезду ещё впереди. Игровая сборка fa56d05, 012 открыта.

2026-10-05, [Run 015](runs/015-original-support-loader/README.md): source flags
найдены в mt payload+0 и перенесены в Rust вместе с plane/gap/quad split.
208 material и 10 plane случаев совпали с исполнением оригинального x86;
Rust проверил 15 трасс / 3651 mt entries. 11 тестов, fmt/clippy/wasm32 прошли.
Далее Base/pr/vt selection и заполнение пространственного дерева. Подключение
к заезду ещё впереди; сборка fa56d05, 012 открыта.

2026-10-05, [Run 014](runs/014-original-spatial-query/README.md): восстановлены
spatial bounds, traversal и cache опорной поверхности; 572 результата Rust
совпали с исполнением оригинального x86. Семь тестов, fmt/clippy/wasm32 прошли.
Далее raw material mapping, построение дерева и подключение к состоянию авто.
Игровая сборка остаётся fa56d05; эта логика ещё вне игрового цикла.

2026-10-05, [Run 013](runs/013-original-support-query/README.md): перенесены
исходные опорные polygon predicates/выбор листа, 36+10 результатов совпали с x86.
Исходный quad split проверен отдельно; timer128/derived counters64Гц прослежены.
Rust tests/fmt/clippy/wasm32 check прошли. Далее загрузка/запрос геометрии сцены
и связь со state/integration; игровой цикл ещё использует прежнюю модель.

2026-10-05, [Run 012](runs/012-original-contact-kernel/README.md): отдельный
Rust-модуль исходной реакции контакта, 468 сравнений с исполнением оригинала,
подтверждённая геометрия точек/нормали и связь углового поля с интегратором.
103 теста nfs-assets, fmt/clippy и wasm32 check прошли. Подключение к игровому циклу
пока впереди; release остаётся fa56d05.

2026-10-04, [Run 011](runs/011-original-physics-audit/README.md): основной метод —
восстановление кода/поведения оригинала, включая старую логику. Приближение sweep
границ убрано из runtime и сохранено отдельно; release остаётся fa56d05.
Идёт трассировка исходных контактов; [аудит](research/original-behavior-audit.md)
перечисляет неподтверждённые solver/ИИ/миссии. 012 не закрыта.

2026-10-05: EDG связан с исходной реакцией автомобиля; 64 случая проверены
исполнением оригинального x86-участка. [Разбор и replay](research/original-collision/response.md).
Пока без переноса в runtime; угловая реакция и полный контракт ещё открыты.

2026-10-04, [Run 010](runs/010-result-continue/README.md): исправлено зацикливание
«Продолжить» после финиша. Factory Driver возвращается к обновлённому списку миссий;
новая попытка снова показывает результат. Исправлен SIM второй миссии 1m01;
браузер подтвердил её старт и сохранение прогресса. Windows/WASM собраны.
Обновите текущее окно через Ctrl+F5,
чтобы сохранить профиль того же адреса браузера. Пользователь прошёл 0M01;
это отдельное свидетельство от незавершённых автоматических заездов Run 008.

Обновлено 2026-10-03. [Run 008](runs/008-wheel-animation/README.md): колёса
вращаются и поворачиваются вокруг центров из CRP; кузов сталкивается с дорогой,
машины — друг с другом. 21 браузерная проверка прошла, Windows/WASM собраны.
Два клавиатурных заезда 0M01 дошли до 5/6 точек без переворота, но не уложились
в исходный лимит. Полная приёмка миссии и соответствие оригинальной физике открыты.

[Run 009 — индекс оригинальных бинарников](runs/009-binary-index/README.md):
перед реализацией ищем функции, строки и связи, фиксируем адреса и доказательства.
Подробные данные версионируются в `research/binary-index/`; команды генерации
и поиска описаны в отчёте. Правило действует для всех исполнителей.

Промежуточный checkpoint: [что работает, ограничения и запуск](runs/007-publish-checkpoint/README.md).

В работе. Создана из 011 без прежних runs. Исправления соответствия карьер оригиналу,
старта Factory Driver, подвески и торможения/заднего хода. [Рабочий план](PLAN.md),
[checkpoint run 001](runs/001-fidelity-repair/README.md).

[Run 006](runs/006-mission-playthrough/README.md): восстановлена опора всей
площадки Skidpad по исходным секторам mt53/mt54. 169 Rust-тестов passed,
Windows/WASM собраны. Полный клавиатурный заезд доходит до 4/6 точек без
падения, но теряет разворот и получает тайм-аут; прохождение ещё открыто.
Последующая диагностика подтвердила переворот: исправлены стабилизатор и лучи
подвески, 172 Rust-теста passed. Заезд всё ещё не пройден; следующий шаг —
неподтверждённая трактовка полей SIM как колеи и источник геометрии колёс в EXE.

Продолжение: [Run 002 — SIM и Quick Race](runs/002-runtime-sim/README.md).
В главном меню Quick Race позволяет выбрать трассу/автомобиль, 1/3/5 кругов и
0/3 соперника; на спринте один заезд. Гараж, деньги и карьерные награды не меняются.
Карьерные заезды и Quick Race загружают оригинальный SIM после модели; отсутствие
или повреждение SIM блокирует старт. Каталог доступных вариантов пока неполный.

```powershell
./scripts/build.ps1 -Iteration 012-campaign-fidelity -Config release -Target all
./scripts/extract-fe-ui.ps1
./scripts/launch-viewer.ps1 -Iteration 012-campaign-fidelity -Mode desktop
```

Снимки 001–011 и local/game неизменны. [Run 005](runs/005-trigger-contract/README.md):
169 Rust-тестов passed, 3 GPU ignored; Chromium 21 checks passed, 0 JS errors/known failures.
Контрольные точки используют восстановленную проверку диагоналей корпуса и условий SCN;
адаптер размеров/позы ещё требует сравнения с оригинальным bounds/contact path.
Исправлен тормозной импульс колёс (8/8 повторов R→вперёд), применён масштаб SCN v4.
Windows/WASM release собраны. Стартовая камера 0M01 больше не закрыта стрелкой L A:
подключены animdefs triggerable и выбор по ссылкам SCN при reset/прохождении триггеров.
Проверены Start→Arrow 1, первый waypoint→Arrow 2 и повтор. Визуальная приёмка оригинала
остаётся открытой: UI ещё не 1:1, правила, spawn и камера требуют дальнейшей проверки.
