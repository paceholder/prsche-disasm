# v2 / 001 — восстановление исходного кода оригинала

Current build and acceptance boundaries are in [STATUS.md](../../../STATUS.md).
[PLAN.md](PLAN.md) owns the active subsystem work and its exact proof commands.
The full recovered game is not yet linked or running. All checkpoints below are
historical; they must not be read as the current build result.

Отдельный compile-corpus checkpoint — [Run274](runs/274-shared-heap-union/README.md):1125candidate
function VAs+19supportobjects,414linknames remain, EXE/DLL отсутствует.
[Run258](runs/258-x86-call-chain/README.md):3original CALLцепочки/36 864native comparisons PASS.
[Run262](runs/262-x86-cpu-primitives/README.md):15 744synthetic CPU cases PASS.
Очередь — [Run275](runs/275-link-frontier-queue/README.md); задачи — [PLAN.md](PLAN.md).

Checkpoint2026-10-16 Run249:1120distinct functionVAs+13supportobjects=1133archive
members/1286822B. Actual link418names/602diagnostics, noEXE/DLL (-24names vs243).
Linkedfrontier87missingFUN/35canonicaloutside/283data/13othercontracts.
26dependencies closed,2newcanonicalTLS/FILEstate contracts exposed.
1125symbolsnapshots reused,8newCOFFs read,7292pinsPASS;0unioncompiles/onearchive-link10.974s.
247:3canonical C++bodies mechanically reused+3exactbindings,4056nativecases,
103originaloraclebytes/52CALLs/293pinsPASS; fullprovidercompositionstillopen.
250/254:23writableoriginalDWORDs,219MEM4/HIGHLOWrefs andnativebits/crosswritesPASS.
248navigationoverclaims corrected251;9screenedcells explicitlydeferred, no guessedallocation.
246isolated x86-to-C++pilot:3existingbodies/76B/28instructions,375808register/CFZFSFOF/
relativeESP/memory comparisonsPASS;one4TU MP6batch,0newacceptedfunctions/noproductionchange.
252coveragecensus:0/85remainingbodies supportedwhole;80needCALL,3tailJMPcases selected.
256automaticqueue refreshed:418linknames ->416tasks,20resolvedrawrows excluded.
Production173 unchanged476full+6partial/game-link267names303refs.
253tailJMP/MissingEntryVMpilot completed:30originalB/6instructions,18432nativecutpoint
comparisonsPASS/49pins/8guards;actualthree-targetJSON retainsisolatedVMpilotprovenance.
No acceptedfunctions/calleeimplementations/nativeFUNbindings ormainqueuefeed.
No activeagent assignments atcheckpoint. Next: originalCALL/stack/RETbackend test with
explicitreturnPC/unknown-target handling, or connectedcanonicalproviderfrom256queue.
FullCPU/memoryloader/imports/gameexecution remainunimplemented.
Detailscommands:runs/249-canonical-and-storage-union and246/256 READMEs; backlog remainsopen.

Historical checkpoints:

Checkpoint 2026-10-15, Run216: **1108 разных native candidate VA** (+14 к206),
959ordinary+149leaf,0overlaps. Actual archive1256244B; raw443names/827refs,
actual432unresolved names/624diagnostics, EXE/DLL нет.
Function names133→126:89missing bodies,36canonical outside union,1compiled-VA contract
mismatch00590290 (existing0arg symbol vs proven2unused caller stackslots); no guessedalias.
Data273→299;7printed CRT contracts.216 reused1107symbol snapshots/531platform pins,
reread1new object,0compilations,6804pins before/after PASS; collector7.473s.
214 restored actualDWORDoutput afterGetCurrentPosition, modular32arithmetic andsignedIDIV;
42PEinstructions,36864nativeinteger-tail asm comparisons+3callbackguardcasesPASS.
207/208/210/211/212 bounded audits PASS;213 reuse navigation queue00537b40.
Production173 unchanged476full+6partial and267game-linknames/303refs.
Все пакеты207–216 завершены; активных назначений/approval ожиданий нет.
Следующий bounded шаг:00590290 proven rawcdecl symbol binding; затем00537b40 callers
и89missing/299data owners/aliases/initializers. Private WORD006b48da:2 mustsharebase+2.
Fullprovider/fault/register/gamefidelity remainopen; countsarenotfullgameremainder.

Historical checkpoints:

Checkpoint 2026-10-15, runs202–206: **1094 разных native candidate VA**
(945 ordinary +149 leaf), +22 к199; пересечений VA нет. Run206: настоящий archive
1238938B; raw423names/800refs, actual413unresolved names/600diagnostics, EXE/DLL нет.
21 прежняя функция закрыта, 9 новых function dependencies выявлены: frontier145→133
(36 canonical owners вне union +97 missing); data241→273, 7printed CRT contracts открыты.
202:00485100 — original9raw32 stores/8bitviews,97PEinstructions и1024nativebitcases PASS;
полное тело/x87/caller register equivalence остаются в backlog. Небезопасный200COFF исключён.
203:21/25 fresh COFF из одного /MP6 batch,0warnings;4 pointer-call failures открыты.
Observed compiler exit/timing потеряны при сбое metadata и остаются null; COFF/source/
include closures и diagnostics проверены отдельно, native batch не повторён.
204: repeatable ordinary+leaf packets, exact pins/cache и overlap guards;206 переиспользовал
1072 symbol snapshots и531 platform pins, прочитал22новых объекта,0компиляций.
205: historical warning/regex inventory — навигация, не semantic acceptance.
Production173 без изменений:476full+6partial, game-link267symbols/303refs.
Пакеты завершены, активных назначений нет. Root README приведён к текущему v2 состоянию.
Следующий шаг: четыре203pointer-call consumers (00540480/005404e0/00554530/0058f1f0)
по original CALL/HighIR; затем97missing functions и273data owners/initializers без stubs.
Команда объединения и ограничения: runs/206-frontier-and-float-union/README.md активной итерации.
Пропущенные проверки: docs/recovery-validation-backlog.md. Full corpus/oracles не повторялись.

Исторические checkpoints:

2026-10-15 checkpoint193–199: **1072 разных native candidate VA** =924ordinary+148leaf,
18 новых относительноa17f569; у28 leaf bodies машинные байты совпадают с оригиналом.
196: один19-TU /MP6 batch4.373с,16new/130reused.197:2COFF, retry только изменённой
004930f0; её15байт совпали полностью. Ghidra/raw storage и inferred types разделены.
199: настоящий1072-member archive1212002B и diagnostic link без компиляции;
raw403names/780objectrefs, actual393unresolved names/581diagnostics.
145FUN:36canonical owners внеunion/109missing;241data и7printed CRT contracts открыты.
198 reusable CLI: несколько disjoint packets, exactSHA/symbol cache, overlapVA reject.
193:2 callee ABI corrections восстановили потерянные CALL inputs;373instructionsPASS.
194:12aliases/21original instructions,8direct token-RAM links (7float4+raw32store);
3FSTP direction discrepancies и unresolved storage/string cases сохранены явно.
Whole original Ghidra project79files/572766909B неизменён; readonly экспорт/копии.
Production173:476full+6partial, прежний game-link267symbols/303refs; игрового EXE нет.
Все193–199 packets завершены, активных назначений/approval ожиданий не осталось.
На checkpoint199 планировался200: bounded per-consumer extern views для7 доказанных float4 aliases,
только changed TUs из189/184 и cachedCOFF; storage owners/initializers не угадывать.
Затем масштабировать token-RAM exporter на подтверждённые классы _DAT/string aliases.
Потенциальные757 _DAT gap bodies — scope исследования, не принятые исправления.
Ghidra escalation передавать координатору сразу; full corpus/oracles не повторять,
пропущенные проверки сохранять в docs/recovery-validation-backlog.md.

2026-10-14: compilation-first192/190:1054 разных Porsche.exe VA дали реальные
x86 COFF:924 общего корпуса +130 thiscall leaves, пересечений0. У27 leaf bodies
машинные байты совпадают с оригиналом. Это candidates, не игровой EXE/fidelity acceptance;
production baseline476full+6partial и game-link173267symbols/303refs неизменны.
184:3312TUs reused;2460 changed covered37batchedcl +12pilot/153priorfailures,
syntax733→923, missing-global errors25306→16818,0lost/0ambiguousfallback.
188:733objects reused/190new,3 /MP6 batches43.416с,0codegenerrors;
384undefinednames/747refs автоматического поднабора.187:127COFF/32fail, only__fltused8refs.
186:1258/1301 inferred RET extents match;42mismatch/1noRET, missingcallargs remainopen.
189 завершён:99exact rewrites/39TUs,1920nativechecks;923→924syntaxpass.
192:923COFFreused/1new00553a70,2.732с/0codegenerrors;190130leafCOFF/29failures.
byte-identical reuse и originalABI/data evidence сохраняются. EXE пока нет.

Текущий результат — [Run173](runs/173-float-transport-integration/README.md): **476 проверенных C++ функций** и6 зарегистрированных частичных consumers.
Общая MSVC Win32 сборка:157 projects /292 unique TUs;137 comparison/alias targets,9 native OS и3 link fixtures.
15 свежих original-x86 отчётов /24560 сравнений, включая8791 повтор в production без observers, плюс4 composed cases; все12 native/link проверок прошли.
Приняты46 новых функций: original float output/parse, lower network transport и network constructors.
00580ad0 повторно проверена через canonical io_find; второй алгоритм и двойной учёт устранены.
Game-link:0 compile errors /267 unresolved symbols /303 references /0 other diagnostics; игрового EXE нет.
Это текущая граница зависимостей, а не полный остаток. RTTI owners и полный004d3420 ещё открыты.
Приоритет2026-10-12: полный собираемый C/C++ граф → первый экспериментальный запуск; глубокие проверки пакетами через backlog.
Проверенные функции и механически восстановленные compile candidates учитываются отдельно; заглушек и guessedalgorithms в игре нет.
SHA-аудит батчем/кешем; измеренный inventory9,3→2,4с относится к156, не ко всему восстановлению.
Маршрут: полный проверенный C/C++ baseline → схема устройства игры → Rust native/web.
Полный игровой запуск, live timer/WM_CLOSE и визуальная приёмка остаются открытыми.

Начато 2026-10-08 по решению пользователя. Предыдущая разработка сохранена
в коммите `dc6b9d8`; 012 незавершена, продолжение её интеграции остановлено.
001–011 не меняются. Игровые данные общие: `local/game`, только чтение.

[Run 001](runs/001-full-corpus/README.md): все 25 модулей экспортированы,
36 587 функций попытаны декомпилятором; 36 583 псевдо-C результата и
4 failed/timeout сохранены. Основной Porsche.exe — 5772/5772 без failed/timeout.
Это полный автоматический корпус обнаруженного кода. Ручные собираемые
исходники появились в Run 002–003; игрового v2 EXE пока нет.

Цель v2 — восстановить игру целиком из бинарников, получить собираемый
исходный код и исходное поведение, затем переносить платформенные зависимости.
Первый исполняемый результат должен быть Windows/x86-оригиналом из
восстановленного кода. Rust/wgpu прежней версии не задают новую архитектуру.

Образец процесса — [SpaceRangersHD_decomp](https://github.com/pakompom/SpaceRangersHD_decomp):
восстановленный исходный код, закреплённый инструментарий сборки,
проверка функций и финального EXE. Его автор сообщает полное совпадение
SHA Rangers.exe; для Porsche такое совпадение ещё не достигнуто.
Метод и тонкости инструментария описаны в
[development.md](https://github.com/pakompom/SpaceRangersHD_decomp/blob/main/docs/development.md).

Расположение:

- `iterations/v2/001-original-recovery`: план, входной манифест, ручной исходный
  код `source/`, проверки и собственные `runs/`, начиная с 001.
- `research/v2/binaries`: полный листинг каждого модуля, автоматический
  псевдо-C, результаты всех функций и явно нераспознанные области.
- `local/builds/v2/001-original-recovery`: отдельная x86 C++ библиотека и
  консольные verification probes; игровой EXE пока отсутствует.
- `local/experiments/v2-*`: изменяемые эксперименты и дополнительные базы Ghidra.

Существующий индекс покрывает 24 уникальных PE. В v2 учитываются также
16-битный NE `clcd16.dll`, алиасы, драйверы, установщики и патчи; их связь
с игровой сборкой устанавливается отдельно. Неизвестный модуль не исчезает
из отчёта. Покрытие/хеши: [reference/binaries.json](reference/binaries.json).

Ступени восстановления различаются: полный экспорт найденного кода,
автоматическая декомпиляция, ручной собираемый исходник, совпавшая функция,
совпавший модуль. Автоматический `decompiled.c` содержит гипотезы Ghidra о
типах/сигнатурах и не является готовым проектом C/C++. Нераспознанные байты,
косвенные вызовы, ошибки/тайм-ауты остаются в измеримом списке работ.

У Porsche.exe PE linker version 6.0; строка `Microsoft Visual C++ Runtime
Library` находится по VA 0x5c1e5c. Это свидетельства для проверки MSVC-era
toolchain, а не доказательство точного компилятора/ключей/библиотек.
Проверять реальные пробные сборки и машинный код до выбора инструментария.

Сначала собираем и проверяем Windows/x86 baseline. Цель полного совпадения
не достигается постобработкой EXE, копированием оригинальных функций в
результат или заглушками неизвестной игровой логики. Когда компоненты игры
восстановлены, отдельный этап адаптирует графику, звук, ввод и ОС под native/web.
Проверенные прежние источники и replay-стенды используются как свидетельства;
аркадные реализации не переносятся автоматически.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/research/export-v2.ps1 -Mode listing
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/research/export-v2.ps1 -Mode listing -ImportNE
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/research/export-v2.ps1 -Mode decompile
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/research/export-v2.ps1 -Mode decompile -NE
py -3 scripts/research/inventory-v2.py --require-listing --require-decompile-attempts
py -3 scripts/research/inventory-v2.py --require-listing --require-decompile-attempts --write-verification
```

NE находится в отдельном проекте; повторный listing использует `-NE` вместо
`-ImportNE`. Первый критерий приёмки 001: все входные SHA учтены,
весь обнаруженный код экспортирован, каждая найденная функция попытана
декомпилятором, ошибки/неопределённые области перечислены. Это ещё не полная
реконструкция игры.

Run 002: пользователь выбрал сразу C/C++, без ассемблерной промежуточной
сборки. Создано [исходное дерево](source/README.md) с адресным каталогом
всех функций и ручными C++ модулями. Переход 0x4b6710 собирается и совпал
с x86 на 175 случаях; callee 0x4b6a50 только записывается в стенде, не восстановлен.
Современный MSVC 19.44.35229 собирает Win32/x86; точная исходная версия
не подтверждена. [Отчёт](runs/002-cpp-startup/README.md),
[граф запуска](reference/startup.json). 1 проверенная ручная функция,
0 байтовых совпадений/целых модулей. Игрового запуска ещё нет.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-v2.ps1
py -3 scripts/research/verify-v2-startup.py
py -3 scripts/research/structure-v2.py
py -3 scripts/research/trace-v2-startup.py
```

[Run 003](runs/003-fe-stream/README.md): восстановлен цельный участок
fe.txt/command-line → FE stream → исполнение записей: девять новых C++ функций,
таблицы команд/значений/действий и исходные глобальные слова. 236 native/x86
сравнений прошли, настоящий fe.txt даёт 23 записи / 188 байтов. Всего десять
проверенных ручных функций; число байтовых совпадений и целых модулей — ноль.
IO/heap/callback callees ещё не восстановлены: их заменяет запись только в
стенде. Общий реестр — source/recovered/functions.json.

```powershell
py -3 scripts/research/export-v2-fe-tables.py
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-v2.ps1
py -3 scripts/research/verify-v2-fe-stream.py
py -3 scripts/research/inventory-v2.py --require-listing --require-decompile-attempts
py -3 scripts/research/structure-v2.py
py -3 scripts/research/inventory-v2.py --require-listing --require-decompile-attempts --write-verification --verification-output iterations/v2/001-original-recovery/runs/003-fe-stream/corpus-verification.json
```

[Run 004](runs/004-original-heap/README.md): восстановлены 11 функций собственного
heap: создание, выделение с двух концов, выбор максимального подходящего блока,
free со слиянием, resize без перемещения указателя и helpers. Все 551 случай
совпали с оригинальным x86 по всей арене после каждой операции. ABI free
уточнён (возврат 1), 236 FE регрессий повторены в новом run; старые отчёты сохранены.
Всего 21 проверенная ручная функция, 0 байтовых совпадений/целых модулей.
OS/CRT/оптимизированные копировщики пока внешние зависимости.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-v2.ps1
py -3 scripts/research/verify-v2-heap.py
py -3 scripts/research/verify-v2-fe-stream.py --report-dir iterations/v2/001-original-recovery/runs/004-original-heap/fe-regression
```

[Run 005](runs/005-original-files/README.md): восстановлены 29 функций
файловых очередей, completion/status, чтения блоками и списков. Все 89
совместных FE/heap/IO проверок с x86 прошли, включая настоящий fe.txt;
551 heap и 236 FE регрессий сохранены в новом run. Всего 50 проверенных
ручных функций. Дополнительный SHA index содержит 3 пропущенных callbacks:
каталог 36 587 automatic + 3 supplementary = 36 590 записей.
Disk/thread/VirtualAlloc/OS/CRT остаются внешними границами; игрового EXE нет.

[Run 006](runs/006-file-device/README.md): ещё 6 функций page wrappers,
list constructors/default key/device init сверены на 474 случаях с x86.
89 совместных проверок повторены с настоящими page wrappers; всего 56 функций.
Каталог 36 587 automatic + 5 supplementary = 36 592 записей.
Worker 0x568530 проиндексирован, но пока не восстановлен.

Следующий пакет — worker/control/completion, thread/wait/events и disk/archive
dispatch, затем привязка к startup 0x4b6a50.
Точные адреса и первый шаг — [в плане](PLAN.md).
