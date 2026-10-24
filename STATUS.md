# Состояние проекта

Current [Run409](iterations/v2/001-original-recovery/runs/409-tapi-modem-integration/README.md):
223 C++ TUs/8 projects compile;195 unresolved names/203 refs,no EXE. Eight original
TAPI/modem bodies741B integrated after21580 nativecases/sequences and14mutations;
555-pin audit PASS. Original callback/resolver/platform composition remains open.
Workers complete. GitHub MCP/search remains deferred until aftercurrentiteration.

Older entries below describe historical checkpoints.

Run332 compiles171 current C++ TUs/8 projects, leaving261 unresolved names/
286 references, noEXE or game execution. Two original constructor/drive bodies
plus the existing canonical search TU enter production.20736 fresh native
cases, six detected mutations and shared433-pin audit PASS; three names close
with no new dependencies. REA333 confirms original allocator/drive/search.
All current assignments complete. Next334/335: original open wrapper/callback
with an extern-only transform-cell contract; storage/aliases/full-system proof
remain open. Root/arena/context and visual/game acceptance remain HOLD.
Older entries below describe earlier checkpoints.

Run328:168 current C++ TUs/8 projects compile,264 unresolved names/289
references, noEXE/game execution. Original resource existence/request and device
routing bodies (4VAs/134B) integrated after19456 native cases. REA325 completed
one persistent session; shared427-pin audit PASS. Compared with324: three old
names closed, three dependencies exposed, four fewer unresolved references.
Next constructor00568260; canonical search graph intake and drive-letter helper
follow. Callback00561ce0 is now indexed but remains evidence-only; transform
state006ad750 and root/arena/context/full-game validation holds remain open.
No unfinished agent assignments. Older entries below describe earlier checkpoints.

PR22 andPR23 are merged with corrected evidence, and GitHub confirms0 open PRs.
PR22 changes canonical input behavior; PR23 is research-only. All review tasks
and commits are complete. Current game324:166 TU compiled,264 names/293 refs,
noEXE or game execution. Full-game and recovery-validation holds remain open.

PR22 merged; PR23 reviewed/fixed for isolated research intake. Final main-tree
audio fixtures30/30+17/17 and strict audits PASS. No audio production integration.
Game324 remains166 TU,264 names/293 refs, noEXE. Contributor licensing agreed.

Run324:166 TUs compiled,264 names/293 refs, noEXE. PR22 closes two names with
no new dependencies; native24, rebuilt1026/13/37/8 and corrected423/324 audits PASS.
PR23 remains isolated; corrected provenance is finalized before research intake.

Current review: PR22 input modes and PR23 audio recovery; game checkpoint321
unchanged pending decisions. Main fast-forwarded to3c5f9b8, which records the
contributor's MIT agreement in LICENSING.md. Earlier pending-consent entries
below are historical for this contributor.

Licensing: MIT for project-owned original work; LICENSING.md records exclusions
and pending agreement for older external contributions. The repository is not
claimed to be uniformly MIT-licensed. Game checkpoint321 remains unchanged.

PR20 merged (final integration c44928c); GitHub confirms closed/merged.
Research-tool corrections verified:107 supported fragments/109568 native
cases,14 generated-code edge cases, three required unsupported-operation rejections.
The original108-fragment result is retained separately; final syntax census5674/
5694 is not game execution. See docs/research/pr20-acceptance-2026-10-20.md.
Game checkpoint321 unchanged:166 TU,266 names/295 refs, noEXE. Remaining resource
file boundaries are documented; no active implementation assignments remain.

2026-10-20: Run321 integrates both original SIMD fill backends. Standalone1440
MMX+3456SSE cases and27648 complete dispatcher/backend chain cases PASS.
All166 C++ TUs compiled;266 unresolved names/295 refs, no EXE/game execution.
No new dependencies/providers. Shared-cache audit PASS;418 unchanged source pins.
PR20 is under separate research-tool review; no lowering backend is in the game.

Продолжение:319 MMX005b0a40 и320 SSE005b0b00 восстанавливаются параллельно;
игровой checkpoint пока318 (164 TU,268 names/297 refs, noEXE).
Координатор готовит321 и общую проверку; результаты ещё не приняты.

REA316 выполнен: scalar/MMX/SSE в одной постоянной MCP-сессии без API,
SHA оригинала и копии до/после совпали. Cold91.38s, repeat38.88ms,
MMX72.89ms/SSE138.05ms. Это анализ, не приёмка SIMD-backends; они ещё unresolved.

Run318, 2026-10-19: проверенный scalar005b0980 теперь в игровой сборке.
1296 native cases/9075 checks (317); actual164 C++ TU/8projects compiled,
268 names/297 refs. Закрыто одно имя, новых нет; batch audit318 PASS,412 pins.
EXE/запуска игры нет. Следом MMX/SSE original backends через REA и native сверку.

2026-10-19: [PR #2–#19 разобраны и закрыты](docs/research/pr-triage-2026-10-19.md):
18 итоговых комментариев, #3 merged, 17 closed без полного merge, 0 открытых PR.
GitHub MCP после перезапуска проверен чтением и записью; прямой HTTP MCP-клиент
работает с локальным токеном. Выборочные принятые тела уже входят в Run314:
163 TU compiled, 269 unresolved names/298 refs; рабочего EXE пока нет.
Основная задача продолжена: REA316 анализирует fill backends в одной сессии,
317 восстанавливает scalar005b0980 со сверкой оригинальных байтов и ABI.
Остаток PR сохранён в docs/recovery-validation-backlog.md. Старые записи ниже
исторические; исправление duplicate root в новом #19 не снимает layout/init HOLD.

Checkpoint 2026-10-19, [Run314](iterations/v2/001-original-recovery/runs/314-pr7-pr9-game-integration/README.md):
PR#7/#9 применены выборочно:0053c290 dispatcher и0059d8e0 resource open,
283 original bytes/37 native cases PASS. Остальные функции этих PR не импортированы.
163 actual game TUs compiled,3 old contracts closed/4 new dependencies,
269 names/298 refs, ноль compiler/other linker errors, noEXE/run; audit314 PASS.
408 source pins,389shared+19delta; data providers/oracle bytes не подменены.
Активных назначений нет; далее original fill backends, сначала005b0980.
GitHub plugin установлен CLI; token/MCP initialization и46tools list проверены.
В текущем IDE-сеансе инструменты ещё не загружены; нужен полный restart VSCode.

Checkpoint 2026-10-19, [Run310](iterations/v2/001-original-recovery/runs/310-pr-intake-integration/README.md):

- PR#3 слит42813f7;21 TU compile PASS. Подключены только00412030/00425780
  и selective PR#4 helpers004a54b0/004a54e0 (Run311),10 native cases/275B PASS.
- Actual game graph161 TUs скомпилированы:4 old contracts closed/8 new dependencies,
  268 names/304 refs, ноль compiler/other linker errors. EXE не получен и не запускался.
  Shared389-pin base+13delta; независимый audit-v2-pr-intake-310.py PASS.
- Остальные19 тел400 не приняты в игровой граф. Whole PR#4 не слит:
  clock401 кэширует изменяемый count,402 дублирует compiled owner290.
- Активных назначений нет. Следующий шаг: original53B005338d0 из400, native slot/
  virtual-call/callback verification. Storage/init и296/301/303 HOLD остаются.

Checkpoint 2026-10-19:18 открытых PR разобраны относительно16638e4.
- [Решения по каждому PR и доказательства](docs/research/pr-applicability-2026-10-19.md).
  Merge/review submission не выполнялись. #19 целиком не принимать: потерянный
  ECX, несовпадающие UI symbols, две root cells и zero heap providers.
- Кандидаты первого приоритета:#3/#4 (9 exact frontier symbols, ещё не приняты).
  Большая часть #5–#11/#17/#18 повторяет canonical owners; переносить выборочно.
- Выполнены Git blob comparison и свежая x86 компиляция/dumpbin411/417/419/420.
  Native suites PR и свежий game build не запускались. Активных назначений нет.
- Игровой результат не изменился:Run309,158 TUs,264 names/300 refs, EXE нет.
  Следующий шаг: bounded intake #3/#4 поверх309 с original ABI/data verification.

Предыдущий checkpoint 2026-10-19, Run309 — готовый результат перед разбором PR.
- Реальный игровой граф: 158 C++ TUs скомпилированы, link264 имён/300 ссылок.
  После298 закрыты pool shutdown005678f0 и disk-slot shutdown005918c0; новых
  unresolved нет. EXE не получен, игра не запускалась.
- 305/307: 248/181 original bytes, по48 native comparisons PASS, существующие
  canonical providers. 304: отдельный heap-slot release72B/960cases, без подключения
  до восстановления parent. Общий source base389pins +3/+6delta, audits PASS.
- REA302/308: CLI и реальная постоянная MCP stdio-сессия PASS, без API.
  Cold86.7s; повтор24ms из snapshot; другая функция65ms в том же соединении.
  IDE/global MCP registration не выполнялась; прямой PID Ghidra не наблюдался.
- 296/301/303: shared root/arena/context layout и исходная инициализация HOLD.
  Сохраняем явные границы; неподтверждённых storage/нулевых providers не добавлено.
Активных назначений нет. Пользователь просит закончить коммит и затем разбирать PR.
310 canonical-binding triage не началась из-за лимита агента; WIP-файлов нет.
Не начинать новые recovery-пакеты перед следующим поручением по PR.
Проверки: audit-v2-disk-game-integration-309.py, audit-v2-pool-game-integration-306.py,
audit-v2-rea-pilot-302.py, audit-v2-rea-mcp-session-308-004.py (scripts/research).


Checkpoint 2026-10-18, Run294: общий формат повторяющихся input/source pins готов.
- v2_shared_manifests.py:immutable content-addressedbase,delta/removals,
  legacyreader,explicitraw/crlf-to-lf modes,existingProofCache;9tests PASS.
- Дваcaptured293buildreports используютодин380-pin manifest;
 118624→59847B pinpayload (-49.55%). Receiptsраздельные;legacyJSONнеизменены.
  Sharedbase читается/разбираетсяодинраз;380sourcehashes вычисляютсяодинраз.
- Независимый294audit PASS;новыхcompiler/nativecommands нет. Это экономия
  повторногоpayload вновыхотчётах, неуменьшениевсегоGitrepo/истории.
- Новыеproducers использоватьсобщимreader; старыеcollectors непереключены
  автоматически. Toolchain/COFF/ownership схемы отдельны, нецентрализованы294.
Главнаяцельтаже:compile/runoriginalgame. Следующий connectedpacket по293:
00628960storage/init и00423840/00482e50/00476170. Активныхагентовнет.

Предыдущая игровая сборка:

Checkpoint 2026-10-18, Run293: свежий source game-target build.
- Все151 C++TUs в8actualprojects скомпилированы;380source/dependency pins до/после
  совпали. Линковка269names/305refs,0compilererrors/0otherlinkerrors;noEXE.
  Это полный текущий targetgraph, не cached173 libs и не ALL_BUILD/probe sweep.
- 291:correctpointer-return004898f0/25B,512nativecases PASS. Old187void-return
  candidate неalias/не+1distinctVA, исключён из новогоgamegraph.
- 292:единственныйcanonical005e57a8owner26→48cells/192Bprefix;4HIGHLOWsites,
  all48initialcells и112native/originalcases PASS. Storagehold закрыт;
  realOSconcurrency иoriginalCRTinitialization остаются открытыми.
- Original005d6e84newline восстановлен вcanonicalapp_main и найден в новомCOFF.
- Independent293audit PASS;oneMP6batch291,oneMP6batch292,gamebuild/MP6.
  Failed292library-path link и293sandboxbuildcheck сохранены; compileнеповторяли
  радиmetadata. ТриC4005 warnings только в неизменённыхplatformsources.
Kong: пользователь предложил пилот, но APIkey/localLLMendpoint не настроены.
ИИ-прогон не выполнялся; инструмент не принят и не отвергнут по качеству.
Продолжаем текущийoriginal-evidence process. Агентыне стартовали (pending_init),
задания прерваны;291/292 завершилкоординатор. Активныхназначенийнет.
Ближайшийconnectedpacket: trace00628960storage/init иcallee00423840(67B),
00482e50(69B),00476170(169B) поиндексу/actualconsumers до реализации.

Предыдущая интеграция:

Checkpoint 2026-10-17: приоритет — игровой EXE и реальная цепочка запуска.
- 285: два normal-entry links без WHOLEARCHIVE и без компиляции,267 имён/
  303 references в обоих. Добавление actual1125-candidate archive274 не изменило
  достижимый frontier; no EXE.18 input pins/22 searched libraries audit PASS.
  Это свежая линковка cached173 graph, не полная пересборка текущих исходников.
- Следующий пакет: original CALL targets и ABI для20 unresolved app_main services,
  начиная с application_main_log, затем startup/rendering. Сначала существующий
  canonical/WIP owner, затем доказанная связь, без заглушек и guessed aliases.
- 284: host обходит original pre-WinMain initializer chain.286 уточнил: у первых
  двух конструкторов нет найденных прямых downstream consumers; это открытая
  проверка эквивалентности, а не доказанный игровой блокер. Не расширять CRT
  восстановление ради этих двух isolated writes без доказанной необходимости.
- 276 завершён:2canonical file-operation тела/2ABI-binding,8 native calls против
  4 original cutpoints PASS. Отдельный пакет, в union274 ещё не подключён.
- 277 завершён:30 976 native comparisons PASS, MOVSX/MOVZX/OR/NOT/Jcc;
  syntax-only25→29/85,0новых принятых игровых функций.
- 279: physical FILE globals footprint;9raw gaps ещё открыты.
- 278/280: прототип caller ABI repair/collector не завершён; только сохранённая
  работа, не применён/не validated.281/282 отложены до начала реализации.
Активных назначений после финализации283/286 нет. Точный следующий шаг —
связать доказанный logging owner с реальным entry target и повторить только
затронутую компиляцию/линковку.267 имён — frontier, не оставшееся число функций.

Предыдущий candidate-archive checkpoint:

Checkpoint 2026-10-17, Run274: **1125 candidate function VAs +19supportobjects**,
1144archive members/1 299 024bytes. Actual link **414names/566diagnostics**, EXE/DLL нет.
Связаны canonicalheapfree+rawbinding+realWin32locks+одинshared64Btable owner.
1125gamecandidateVAs не включают2modernplatformadapters (support). Verified production
Run173:476full+6partial без изменений.7626pins PASS/1142cachedsymbols/2newCOFFs,
0compiles/onearchive-link8.827s. Source/archive/actualdiagnostics —runs/274.

266:10actualrawfreeCOFF callers/fullDWORD cdecl RET0,EAX1/nativeidentity PASS.
268:2existingWin32adapters,64real recursive-lock cycles/256calls,204pins/8guards PASS.
267/270/271:56746HIGHLOW/30refs +4candidate spans510B/167instructions recovered;
newnavigationindex15pins/7guards PASS,entry/liveness unproved, oldindices untouched.
272:STORAGE-only64B/exactcanonicaldeclaration +rawindexed16slotview shareonebase;
3actualrawDIR32sites boundedAND0f/MEM4,zero/crosswrites/bitcopy/nativeaudit PASS.
Datafootprint proof independent ofunprovedcandidatefunctionliveness; those functions
не реализованы/приняты. Preceding006b4b20..006b4f1f endpointer/relativelayout remainsHOLD.
275:414actualnames→412research tasks,24resolvedrows excluded;159pins/6guards PASS.
258 originalCALLchains36864native cases;262syntheticCPU15744cases PASS;
260syntax25/85,notsemanticacceptance.595d0MOVSX/5a4c05layout holds сохраняются.

Исторический план после275: files.cpp/native backend packets завершены в276/277;
595d0 caller-return hold остаётся открытым. Ближайший приоритет изменён по285:
связь существующего корпуса с реальным entry graph, см. текущий checkpoint выше.
Команда прежней wholearchive union — runs/274/command.json (collector268).

Предыдущий checkpoint:

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

Historical checkpoint216:

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

Historical checkpoint206:

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

Исторический checkpoint (заменён результатом206):

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

Текущий результат — [Run173](iterations/v2/001-original-recovery/runs/173-float-transport-integration/README.md): **476 проверенных C++ функций** и6 зарегистрированных частичных consumers.
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

2026-10-14:173 опубликован6952020.177 direct C++ body004d3420 компилируется
с открытыми внешними seams;178 завершён707/5772 C++ syntax-pass.
179 экспортирует полные declarations/types;180 исправляет точные integer p-code
helpers;181 строит реальные COFF из кандидатов. Это isolated experiments,
accepted function count и production game-link остаются173; игрового EXE нет.
Проверки пакетами, неизменившийся178 baseline повторно не гонять.

2026-10-08: активна [v2/001-original-recovery](iterations/v2/001-original-recovery/README.md).
v1 сохранена и запушена: dc6b9d8; 012 незавершена и её интеграция остановлена.
Полные листинги 24 PE + 1 NE экспортированы: 36 587 функций, 2 827 310 инструкций.
Границы нераспознанного кода/данных/паддинга учтены (3 199 385 байтов);
это не готовые исходники. Все функции попытаны декомпилятором: 36 583 псевдо-C,
4 failed/timeout, 5639 функций с предупреждениями. Основной Porsche.exe:
5772/5772 псевдо-C, без failed/timeout. Проверка полного инвентаря прошла.
Сразу C/C++ по выбору пользователя: адресный каталог всего корпуса,
porsche_original.lib и отдельные консольные стенды (Windows x86).
Run 002: startup 0x4b6710 сверён на 175 случаях. Run 003: 9 функций чтения
fe.txt/аргументов, создания и применения FE stream сверены на 236 случаях.
Сравнивались записи, глобальные поля, callbacks и порядок file/heap вызовов;
внешние службы — записывающие границы стенда, ещё не восстановленные callees.
Настоящий fe.txt даёт 23 записи / 188 байтов. Run 004: 11 новых C++ функций
heap init/alloc/free/resize и helpers; 551 случай совпал с x86 по всей арене
после каждой операции. Повторные 236 FE регрессий прошли; free возвращает 1.
Run 005: 29 функций файловой очереди/completion/chunk read и списков;
89 совместных FE/heap/IO сравнений с x86 прошли. 551 heap и 236 FE регрессий
повторены в новом run. Всего 50 проверенных ручных функций; библиотека
содержит реальный FE/heap/IO frontend. Disk/thread/VirtualAlloc/OS/CRT/SIMD
ещё внешние; в стенде completion немедленный, это не исходный scheduler.
Три пропущенных callbacks добавлены в SHA-guarded index: каталог 36 590 записей,
из них 36 587 automatic. 0 байтовых совпадений/модулей; игрового v2 EXE нет.
MSVC 19.44.35229 доступен; точный исходный compiler/CRT/flags не подтверждён.
Worker/thread/wait/events и disk/archive backend открыты; после них совместная
сверка с исходным scheduler и startup 0x4b6a50.
Run 006: 6 новых C++ page/list/device startup функций, 474 сравнения с x86
и 89 совместных регрессий прошли. Настоящие page wrappers включены в FE/heap/IO;
Win32 APIs остаются записывающими границами. Всего 56 проверенных функций,
каталог 36 592 записей (5 supplementary). Worker 0x568530 проиндексирован,
его loop/thread/wait/events/disk ещё открыты. Следом worker jump table/dispatch.
Отчёт: runs/006-file-device внутри v2/001. Визуальная приёмка открыта.
Пути: iterations/v2/001-original-recovery, research/v2/binaries,
local/builds/v2/001-original-recovery. Исполнитель — координатор.

2026-10-08: пользователь попросил закоммитить текущее состояние и перейти
к отдельной v2 с полным восстановлением бинарников. 012 сохраняется незавершённой.
Run 020: 47 исходных four-wheel loops / 188 состояний совпали с Rust.
221 tests (3 ignored), fmt/clippy/wasm32/native/web и 3 Quick Race прошли.
Общий browser прогон Run 020 не завершён; force adapter/binding открыты.
Run 019 запушен: 13b70f1. Дальнейшие игровые правки 012 остановлены.

Run 019, 2026-10-06: общие polygon+EDG tree и wheel support работают в игре.
3 дерева / 27 EDG queries, 8 body selections, 45 scene-dependent responses,
5 height / 29 material-state cases совпали с x86. 220 tests (3 ignored),
fmt/clippy/wasm32, native/web, 3 Quick Race и 21 browser check прошли.
Body/wheel APIs ещё не заменяют force adapter. Остаток: исходные поля,
full 0x499a70/cadence, special/car-car, UI/миссия/визуальная приёмка.
012 открыта, активных исполнителей нет; координатор завершил частичный код
после лимита worker. Запуск: local/builds/012-campaign-fidelity/Launch-desktop.cmd.

Run 018 внутри 012, 2026-10-06: восстановленные type-1 опоры подключены
к реальной загрузке трасс и четырём колёсам. Ordinary Base filter, retained
flag-1 loader, mt/vt/df/pr, дерево, два кэша и plane query работают в заезде;
RD* и Skidpad mt53/54 подбор исключён. На 15 трассах 237 395 полигонов.
9 loader / 10 owner случаев совпали с x86; 215 Rust-тестов прошли (3 ignored),
fmt/clippy workspace -D warnings, wasm32 и native+web release прошли.
Quick Race Skidpad/Alps/Canyon: реальные кэши четырёх колёс, нагрузка, движение,
высота проверены; 21 общий browser check прошёл. Отчёт/команды/кадры:
runs/018-original-support-runtime. Запуск: local/builds/012-campaign-fidelity/Launch-desktop.cmd.
Силы подвески и body response пока прежний adapter; prepared-contact kernel
не подключён. Type-2/box/cylinder в общем дереве, mutable Base ordinal и
полный original wheel state/forces остаются незавершёнными; 012 открыта.
Назначения исполнителей завершены. Следующий конкретный шаг координатора:
смешанная сцена → original 0x499a70 state/alternate height/force consumers →
геометрия 0x495020 и response 0x494000. Сверка реальной езды с оригиналом обязательна.

Run 017 внутри 012, 2026-10-05: исходные type-1 insertion/split и cached
support query соединены в отдельном Rust-модуле. 38 callback / 304 predicate
cases, 36 полных деревьев и 72 полных запроса совпали с исходным x86. Original
allocator/vector code исполнен на заданной арене; Win32 locks — один поток.
20 Rust-тестов, fmt/clippy -D warnings и wasm32 прошли. Источник/команды —
runs/017-original-support-insertion. Runtime fa56d05, 012 открыта.
Найден special producer: animdefs.txt → 0x47eba0 → 0x628ba0; 0x47ed90 на
неизвестном ненулевом Base tag возвращает первый ENDW/kBox, а не null.
Следом координатор: полный polygon loader 0x475543..0x47591e, original
special lookup/parser и Base ordinal producer, затем state/cadence автомобиля.
Цель подключения в 012 — первая трасса с исходными опорами и контактом.

Run 016 внутри 012, 2026-10-05: Base/pr readers и child constructor дерева
перенесены из оригинальных consumers в отдельные Rust-модули. 317 primitive
payload cases / 2536 helper outputs, 60 Base и 256 child cases сверены с x86.
Rust проверил 15 трасс: 74 300 pr / 16 591 Base. 16 тестов, fmt/clippy -D warnings
и wasm32 check прошли; источник/команды — runs/016-original-support-assembly.
Исполнитель по insertion остановлен лимитом; source export сохранён, child
constructor завершён координатором. Активных назначений нет. Сборка fa56d05,
012 открыта; новые модули пока вне игрового цикла. Следом координатор:
0x484320 + type-1 vtable+4/+8 callbacks → insertion replay; затем полный loader
(ordinal producer/special/alternate/degeneracy) и связь с state/cadence авто.

Run 015 внутри 012, 2026-10-05: material flags = mt payload+0 доказаны;
208 x86 material cases и 10 plane/gap cases проверены, перенесены в pure Rust.
Rust независимо проверил 15 трасс / 3651 mt entries; 11 тестов, fmt/clippy
и wasm32 check прошли. Источник/команды — runs/015-original-support-loader.
Исполнители завершили работу. Runtime пока прежний (fa56d05), 012 открыта.
Следом координатор: Base/pr/vt selection в0x4750b0, insertion0x484ae0/483bd0,
затем сцена → state автомобиля и исходный cadence; приближений не подключать.

Run 014 внутри 012, 2026-10-05: исходный spatial bounds/traversal/cache
перенесён в pure Rust. 572 результата совпали с исполнением оригинального x86;
семь Rust-тестов, fmt/clippy и wasm32 check прошли. Source/fixtures — Run 014.
Путь owner → material flags установлен; raw mt offset ещё неизвестен.
Runtime не подключён, сборка fa56d05.
Ближайший шаг: raw flags linkage, plane/split loader, построение дерева
и связь с состоянием авто. Исполнители завершили работу.

2026-10-05, Run 013 внутри 012: исходные support polygon predicates и выбор в
готовом листе перенесены в Rust. 36+10 результатов сверены с оригинальным x86;
10 quad-split случаев также исполнены. Три Rust-теста (включая 468 контактов),
fmt/clippy и wasm32 check прошли. Выбор/загрузка листа сцены ещё не подключены.
Прослежен источник timer128/derived counters64Гц; cadence интеграции остаётся открытой.
[Run 013](iterations/012-campaign-fidelity/runs/013-original-support-query/README.md).

2026-10-05, Run 012: исходная реакция подготовленного контакта перенесена
в отдельный Rust-модуль; 468 состояний совпали с исполнением исходного x86.
Восстановлены ветка поворота, её Y-поле и геометрия точек/нормали. 103 теста
nfs-assets, fmt/clippy и wasm32 check прошли. К игровому циклу ещё не подключено:
нужны полный запрос сцены и cadence исходного интегратора. Сборка остаётся fa56d05.
[Отчёт и команды](iterations/012-campaign-fidelity/runs/012-original-contact-kernel/README.md).

2026-10-04, Run 011: пользователь выбрал восстановление оригинала вместо
подбора похожих реализаций во всех подсистемах, включая существующий код.
Sweep сохранён отдельно и снят с runtime; в release не попадал.
Последняя игровая сборка/коммит — fa56d05. 012 открыта: физика/UI/миссии
ещё требуют оригинальных consumers и приёмки. 013 (повреждения + рынок б/у)
пока только план. Правила закреплены в AGENTS.md и .cursor/rules.

2026-10-05: доказана цепочка EDG → пространственный запрос → 0x494000.
64 дифференциальных случая участка реакции прошли на исходных x86-инструкциях
в Unicorn; геометрия задана фикстурой, угловой callback отключён. Результаты:
[разбор реакции](iterations/012-campaign-fidelity/research/original-collision/response.md).
Игровой код не менялся; далее угловая реакция и полный контракт контакта.

2026-10-04: пользователь подтвердил прохождение первой Factory-миссии, но
«Продолжить» повторно открывает результат. Run 010 исправляет JS lifecycle;
UI regression и пять Factory Rust-тестов прошли. Windows/WASM собраны.
Chromium: Continue → список → 1m01 → реальный старт, сохранение после reload,
fail/retry без цикла, 0 JS errors. Финиш задан тестовой фикстурой; проходимость
1m01 не заявляется. Исправлен её SIM: boxster.sim (отсутствует) → boxster25.sim.


Обновлено 2026-10-03. Активна **012-campaign-fidelity** из завершённой 011.
Текущий план: [iterations/012-campaign-fidelity/PLAN.md](iterations/012-campaign-fidelity/PLAN.md).

## Активная 012: исправление соответствия оригиналу
Run 008: реальные wheel pivots/радиусы/mesh groups из CRP подключены к физике
и рендереру. Работают вращение/руление колёс, ход подвески игрока, контакт кузова
с трассой и столкновения player/AI/AI. CPU: 184 passed; Windows/WASM собраны.
Chromium: 21/21 общих проверок, wheel pivots/анимация и столкновение Quick Race
прошли; оба заезда 0M01 — 5/6 точек без переворота, затем тайм-аут. Коммит 9efb71c.
Run 009: версионируемый индекс оригинала готов в `research/binary-index/`:
25 PE-путей / 24 уникальных бинарника, 36 510 функций и 1 274 749 ссылок.
NE-библиотека clcd16.dll явно не покрыта; полнота семантики и косвенных вызовов не заявляется.
Правило поиска по индексу перед реализацией закреплено в AGENTS.md и .cursor/rules.
Предыдущий опубликованный промежуточный checkpoint: cb07500.
Предыдущий опубликованный checkpoint по запросу пользователя:
[Run 007](iterations/012-campaign-fidelity/runs/007-publish-checkpoint/README.md).
Quick Race, source SIM, SCN-триггеры/стрелки и исправления поверхности собраны;
полное прохождение 0M01 и оригинальная физика НЕ приняты. Следующий результат —
одна миссия от старта до корректного итогового экрана с короткими коммитами.

Текущий checkpoint 2026-09-29: исправлены обратный знак стабилизатора и геометрия
луча подвески; 172 Rust-теста passed, 3 GPU ignored, Clippy/fmt чисты,
Windows/WASM собраны. Полный заезд attempt-5 по-прежнему переворачивается после
2-й точки; коллизии кузова с землёй нет. Найдена неподтверждённая трактовка
SIM 0x10c/0x110 как колеи: Boxster runtime получает принудительный 1 м.
Следующий приоритет — EXE consumers и источник геометрии колёс, затем повтор
миссии и result/penalty flow. Подробности и все попытки — в Run 006 ниже.

Предыдущие checkpoints Run 006:
Run 006: исправлен провал после первой точки — подключены все исходные сектора
Skidpad mt53/mt54 вместо одного MESH04; исключено одноимённое здание.
169 Rust-тестов passed, 3 GPU ignored; Clippy/fmt чисты, Windows/WASM собраны.
Реальный повтор прошёл 4/6 точек без падения и JS-ошибок, затем тайм-аут на
развороте; профиль не изменился. Полное прохождение ещё не подтверждено.
Далее — телеметрия разворота (управление/физика), затем result/penalty flow.
Attempt-3: кузов переворачивается (up.y до -1). Обнаружен и исправлен обратный
знак сил стабилизатора; тест до правки воспроизводит усиливающий крен момент.
Проверка исправления в полном заезде ещё выполняется.
2026-09-29: attempt-4 всё ещё переворачивается. Исправлена следующая причина:
неверное расстояние до земли и ложный контакт перевёрнутых колёс. Новый raycast
подвески проверяется на конечных треугольниках, наклоне и границах grid cells.
[Run 006](iterations/012-campaign-fidelity/runs/006-mission-playthrough/README.md).

Run 005 проверен 2026-09-28: runtime использует восстановленный shape1 predicate —
диагонали корпуса, height band, source direction/speed constraints, type-based End.
169 Rust-тестов passed, 3 GPU ignored; Clippy/fmt/UI чисты; Windows/WASM собраны;
Chromium 21 checks, 0 errors/known failures. Срабатывание первой точки корпусом,
gate count и reset проверены реальным W/R вводом; кадр после точки осмотрен.
Размеры берутся из видимой модели, pose/velocity из sim; exact original selected
bounds/body origin/contact correction ещё не доказаны. Полное прохождение 0M01,
штрафы/результаты и UI 1:1 открыты. Активных назначений нет, агенты остановились по лимиту.
[Run 005](iterations/012-campaign-fidelity/runs/005-trigger-contract/README.md).

Run 004: animdefs.txt triggerable и упорядоченные ссылки SCN подключены к runtime.
По восстановленной reset-цепочке Start выбирает Arrow 1, проезд первого триггера
переключает Arrow 2, повтор возвращает Arrow 1. L A остаётся скрытой; стартовый
кадр осмотрен, перекрытие камеры устранено. 166 Rust-тестов passed, 3 GPU ignored;
Clippy/fmt/UI чисты, Windows/WASM собраны; Chromium 21 checks, 0 errors/known failures.
Оригинальные условия прохождения, spawn/камера и UI 1:1 ещё не доказаны.
[Run 004](iterations/012-campaign-fidelity/runs/004-scenario-arrows/README.md).
Назначений агентам нет. Следующие записи — история проверок предыдущих runs.

Run 003: исправлена численная смена знака omega от тормоза в physics/tire.rs.
До исправления 2/4 сценарных повторов не возвращались из R, после — 8/8 passed.
Подключён масштаб SCN v4: 162 Rust-теста passed, 3 GPU ignored; Clippy/fmt/UI чисты,
Windows/WASM собраны. Chromium: 20 checks passed, 0 errors, 0 known failures.
Кадр 0M01 просмотрен: L A / ARW1 по-прежнему закрывает камеру. Восстановлена цепочка
animdefs.txt triggerable → initial hide → trigger-linked selection; в runtime она
пока не подключена, первая активация требует дальнейшего реверса. Назначений агентам нет.
[Run 003](iterations/012-campaign-fidelity/runs/003-factory-diagnostics/README.md).
Run 001/002 ниже сохраняют исторические checkpoint, включая уже исправленный сбой.

- Замечания пользователя: чужой интерфейс, смешанное оформление карьер, неправильный
  Factory Driver/старт миссий, подлёты, отсутствие заднего хода после торможения на S.
- Checkpoint 2026-09-26: T01–T04 частично выполнены, итерация остаётся открытой.
  Исправлены первый контакт, фиктивный пол, плечи осей и S тормоз→стоп→задний ход (W симметрично).
  Калибровка на реальных SIM: 356 — 54.7 м, Boxster — 43.3 м со 100 км/ч, без опрокидывания.
  Для 0M01 добавлены SCN Start, опора MESH04 и waypoint 1–5→End; общий круг LSP больше не даёт
  победу Factory, неподдержанные сценарии не получают награду. Правила пока реконструированы.
- 151 Rust-тест passed, 3 GPU ignored; Clippy/fmt/UI-тесты чисты. Release Windows/WASM собраны.
  Chromium: 14 автоматических проверок, 0 ошибок, включая руль и удержание S/W; см.
  [Run 001](iterations/012-campaign-fidelity/runs/001-fidelity-repair/README.md).
- Визуальная приёмка НЕ пройдена: на кадре 0M01 камера перекрыта геометрией. Точный spawn offset,
  правила оригинала, UI 1:1 (комнаты/шрифты/слои) и реальное прохождение ещё не подтверждены.
  В продолжении Run 002 добавлена загрузка SIM в web runtime, проверка повреждённых файлов
  и исправлена ссылка Boxster на `boxster25` по nfs5.car. Native CLI пока использует таблицы.
- Run 002: добавлен Quick Race с выбором трассы/автомобиля, 1/3/5 кругов и 0/3 AI,
  без карьерных наград. Windows/WASM собраны, 157 Rust-тестов passed, 3 GPU ignored;
  Clippy/UI чисты. Финальный Chromium: 19 checks passed, 0 JS errors и 1 known failure
  (нестабильный R→вперёд Boxster в 0M01). Quick Race S/W проходит; общий verifier не green.
- Исправлены знак поворота кинематического AI и lookahead спринта. Аудит 15 трасс теперь
  измеряет прогресс вдоль пути: раньше проверялось удаление от пути >50 м, так что прежний
  pass не доказывал следование маршруту. Исправленный аудит прошёл. AIS/AI-таблицы,
  финиш спринта и исходный состав участников кампании остаются открытыми.
- UI разделяет карьеры и частично следует LAY 640x480. Извлечение 119 PNG воспроизводимо через
  `scripts/extract-fe-ui.ps1` в `local/derived/fe-ui`. Повторный
  [Computer Use](docs/computer-use-recovery.md): `sky.list_apps()` → `spawn EPERM`.
  Назначений агентам нет; результаты и команды — [Run 002](iterations/012-campaign-fidelity/runs/002-runtime-sim/README.md).
- Снимки 001–011 неизменны. Прежние отчёты ниже — история проверок, а не доказательство
  полного сходства с оригиналом; соответствие 011 оспорено замечаниями пользователя.
- Звук/кокпит отложены до исправлений карьер и управления.

Полный перенос с обеими карьерами: [дорожная карта](docs/roadmap.md).

## Завершённая итерация: 011-factory-driver
- [Run 001](iterations/011-factory-driver/runs/001-factory-driver/README.md):
  - T01: бинарный реверс всех 34 миссий `nfs5.fac` и языковой базы `festrings.csv`.
  - T02: движок испытаний `factory_driver.rs` в `crates/nfs-game`: детекторы трюков `StuntDetector` (180° slide, 360° spin, reverse J-turn, слаломные штрафы, контроль повреждений кузова).
  - T03: система карьерного роста пилота (Applicant -> Junior Test Driver -> Test Driver -> Senior Test Driver -> Chief Test Driver -> Master Ace) и наградные автомобили ('78 911 Turbo 3.3, '73 Carrera RS 2.7, '99 911 GT3 Factory Edition).
  - T04: модальное окно `#factoryMissionsModal` с фильтрами уровней сложности Tier 1–3, репликами шеф-инструктора Рольфа, статусами и лучшими временами.
  - T05: устранение дефектов кампаний и аутентичность интерфейса:
    - Аутентичная игровая оболочка Need for Speed: Porsche Unleashed (2000) по макетам `bottombar.lay`, `singleplayer.lay`, `factorydriver.lay`: полноэкранный шелл, верхний брендовый хедер PORSCHE, статус-бар с балансом CR и рангом, левая колонка рубиновых пилюль `.nfs-pill-btn`, нижняя панель с кнопкой ESC и светящейся овальной кнопкой RACE (`racebutton.fsh`).
    - Аутентичный экран брифинга миссий (`missionbrief.lay`): реальное фото Рольфа (`people_rolp.png`), технические чертежи заданий (`map_*.png`, 34 схемы) с расстановкой конусов и стрелками.
    - Аутентичный круглый щиток приборов Porsche в заезде (`hud.fsh`): аналоговый тахометр с redline (6.5–8k) и стрелкой, янтарный цифровой спидометр (км/ч) и индикатор передачи.
    - Устранение ошибки покупки авто (`missing or invalid 'version'`).
    - Загрузка настоящих 3D-моделей Porsche (`.crp`/`.fsh`) и параметров симуляции в сюжетных заданиях вместо коробчатой заглушки.
    - Исправление масштаба 1:1 (x5.0) и разворот модели на 180° вперед по ходу трека.
    - Исправление трассы Canyon и заземления машины со старта Evolution.
  - T06: 142 теста Rust passed across workspace, Clippy `-D warnings` чист, Rustfmt чист, Playwright Chromium E2E (12/12 checks, 0 errors), сборка release native и WASM.
- Задачи T01–T06 завершены, тег `iteration-011`.

## Завершённая итерация: 010-evolution-career
- [Run 001](iterations/010-evolution-career/runs/001-evolution-career/README.md):
  - T01: бинарный реверс турниров (`nfs5.trn`), запчастей (`nfs5.prt`), цен и износа.
  - T02: турнирный движок Evolution с эпохами (Classic, Golden, Modern), зачётной системой очков (10-6-4-3-2-1), прогрессией этапов и призовыми.
  - T03: автосалон новых автомобилей и рынок подержанных (Б/У) с динамическим пробегом и остаточной стоимостью.
  - T04: каталог запчастей и тюнинга, влияние на физику, износ узлов и процедура ремонта.
  - T05: полный стресс-тест физики на всех 15 трассах: 0 туннелирований на скоростях до 252 км/ч, проверка полотна и удержания.
  - T06: интеграция UI, 138 тестов Rust passed, clippy `-D warnings` чист, fmt чист, Playwright Chromium E2E (0 errors), сборка native и WASM.
- Задачи T01–T06 завершены. Документированы скрипты в `docs/scripts.md`.

## Проверенный результат

### 009-game-shell
- [Run 001](iterations/009-game-shell/runs/001-controls-regression/README.md):
  - T00: исправление инверсии руля в 6 DOF физике (`vehicle.rs`), отрицательный угол для правого поворота колёс при направлении -Z.
  - Регрессионный стенд: воспроизведение бага до исправления, 21 тест `porsche-viewer` зелёный, web input checks, Playwright тест знака курса.
- [Run 002](iterations/009-game-shell/runs/002-game-shell/README.md):
  - T01: независимый крейт `crates/nfs-game`, архитектурная машина экранов `Screen`, идемпотентная обработка результатов и защита от гонок загрузки.
  - T02: профиль игрока `PlayerProfile` (11 000 CR, гараж, ранги), атомарное сохранение `.tmp` -> rename на диске, `localStorage` и JSON импорт/экспорт в Web.
  - T03: бинарное исследование `nfs5.trn` и `nfs5.fac`, парсеры турниров и миссий в `nfs-formats/src/career.rs`, отчёт [`research/first-events-evidence.md`](iterations/009-game-shell/research/first-events-evidence.md).
  - T04: сквозной запуск первых карьерных событий: Evolution 356 Challenge (Canyon, Monaco 1; взнос 2 000 CR, приз 4 500 CR) и Factory Driver 0M01 (Skidpad, Boxster, лимит 32.0 с).
  - T05: базовый звук: WebAudio синтезатор (RPM, визг шин, сигналы отсчёта/финиша) и WinMM beeper для native; модальные окна брифинга, профиля и результатов.
  - 129 Rust-тестов passed, clippy `-D warnings` и fmt чистые, release native/WASM собраны, 10 проверок Chromium WebGPU пройдены.
- Задачи T00–T05 завершены, код закоммичен, тег `iteration-009`.

### 008-race-loop
- [Run 001](iterations/008-race-loop/runs/001-race-loop/README.md):
  - T01: конечный автомат гонки (`RaceSession`): состояния `Loading`, `Countdown(3..2..1..GO)`, `Racing`, `Paused`, `Finished`, `Results`, секундомеры, время круга, лучший круг (`best_lap`), дельты сплитов.
  - T02: маршрутизация и чекпоинты (`TrackCourse`): реверс-инжиниринг бинарного формата EA Canada `.lsp` (Line Spline Path), 30 файлов трасс валидированы со 100% успехом, автоматическая классификация кольцо/спринт (порог gap < 60м), ворота чекпоинтов с защитой от срезок, детекция движения против хода («Wrong Way»).
  - T03: столкновения с барьерами (`BarrierCollider`): отскок от кромок полотна `.edg` с коэффициентом восстановления 0.35 и касательным трением 0.75, предотвращение вылета за пределы трека.
  - T04: соперники ИИ и стартовая решётка (`AiOpponent`): следование траектории с чистым преследованием (pure pursuit) и адаптацией скорости под кривизну $v = \sqrt{a_{lat}/\kappa}$, профили на основе `.ais`, шахматная стартовая решётка до 8 машин, живая таблица позиций по пройденной дистанции.
  - T05: гоночный HUD и экран результатов: бейдж позиции (P1/4), счетчик кругов (Lap 1/3), секундомер, таймер лучшего круга, пульсирующий баннер обратного отсчёта, предупреждение Wrong Way, модальное окно финиша в Web и телеметрия в заголовке Native окна.
  - T06: сквозной integration тест (`tests/race_integration.rs`): 115/115 тестов passed; чистые `fmt` и `clippy -- -D warnings`; полная сборка release native и WASM.
- Задачи T01–T06 завершены, код закоммичен, тег `iteration-008`.

### 007-physics-simulation
- [Run 001](iterations/007-physics-simulation/runs/001-physics-simulation/README.md):
  - T01: 6 DOF Rigid Body динамика (`rigid_body.rs`): масса, моменты инерции, кватернион ориентации, локальные/мировые преобразования, численный интегратор с подшагами $\le 1/240$ с.
  - T02: силовая установка и КПП (`powertrain.rs`): 21-точечная сетка крутящего момента с шагом 500 RPM из `.sim`, динамика оборотов маховика (холостой ход, отсечка, сопротивление вращению), передаточные числа и главная передача.
  - T03: 4-колёсная независимая подвеска (`suspension.rs`): лучевые запросы к spatial grid `surface.rs`, силы пружин, амортизаторы сжатия и отбоя (bump/rebound), стабилизаторы поперечной устойчивости, динамический перенос веса кузова.
  - T04: шинная модель и тормоза (`tire.rs`): продольный (slip ratio) и поперечный (slip angle) увод шин, эллипс трения Кулона, коэффициенты `front_grip`/`rear_grip` из `.sim`, статический замок тормозов при остановке.
  - T05: калибровочный стенд (`calibration_bench.rs`): 102/102 unit/integration тестов passed; битовый детерминизм подтверждён; воспроизведение потока управления из `replay.rpl`; разгон и торможение 356A (0-100 за 14.37 с, 100-0 за 43.9 м) и Boxster 2.5L (0-100 за 7.83 с, 100-0 за 41.5 м).
  - T06: интеграция в просмотрщик (desktop/web): переключение на лету клавишей `M` / кнопкой UI между 6 DOF симуляцией и аркадным прототипом; телеметрический HUD со спидометром, тахометром, передачей, перегрузкой $g$ и реакцией колёс; чистые `fmt` и `clippy -- -D warnings`; полная сборка native и WASM.
- Задачи T01–T06 завершены, код закоммичен, тег `iteration-007`.

### 006-game-systems
- [Run 001](iterations/006-game-systems/runs/001-game-systems/README.md):
  - T01: PE-карта `nfs5.exe`, функции загрузки `.sim` (`0x0049c750`), `.ais` (`0x0049ca30`), масштабирующие коэффициенты. См. [`simulation-evidence.md`](iterations/006-game-systems/research/simulation-evidence.md).
  - T02: парсеры `SimCar` (328 байт) и `AisCar` (304 байта) в `nfs-formats/src/sim.rs`. Валидация 88/88 `.sim` и 22/22 `.ais` файлов.
  - T03: архитектура сохранений `.sav` (связный список секций `0x0065b634`), парсеры каталогов `nfs5.car` (109 авто), `nfs5.trk` (15 трасс), `nfs5.fac` (34 миссии Factory Driver) в `nfs-formats/src/career.rs`. См. [`career-evidence.md`](iterations/006-game-systems/research/career-evidence.md).
  - T04: реверс детерминированных реплеев `replay.rpl` (8 суб-сэмплов на тик, RLE-поток ввода, заголовок 15 908 байт на 8 авто), парсер `ReplayFile` в `nfs-formats/src/replay.rs`, открытие 500-RPM сетки крутящего момента, baseline стенд в `local/experiments/bench/sim_baseline.json`. См. [`telemetry-evidence.md`](iterations/006-game-systems/research/telemetry-evidence.md).
  - T05: интеграция, 83 unit-теста passed, `fmt` и `clippy` (`-D warnings`) чистые, полная сборка `build.ps1 -Target all` (native + WASM) выполнена без ошибок.
- Итерация завершена, закоммичена, тег `iteration-006`.

### 005-unified-driving
- [Run 001](iterations/005-unified-driving/runs/001-unified-driving/README.md):
  - Общий desktop/web запуск (`scripts/launch-viewer.ps1`), три `Launch-*.cmd`; desktop — web/WASM в отдельном окне Edge/Chrome. Native CLI сохранён, добавлен каталог без аргументов и непрерывный ввод по кадрам. Пользователь подтвердил интерактивную работу.
  - Web UI: Esc скрывает/возвращает меню, H выключает приборы, диагностика отдельно.
  - Прототип: плавный руль, ограничение угла по скорости, максимум 240 км/ч, подшаги ≤1/120 с и фильтр кромок по высоте. Это не оригинальная физика.
  - CPU spatial grid по статическим RD* треугольникам до batching: высота/нормаль, выбор ближайшего допустимого уровня и исходный article/primitive/triangle ID. Все 15 трасс прошли centroid audit (109 707 треугольников), худшая ошибка 0.000164 м.
  - 72 unit tests passed, 3 специальных GPU tests ignored; fmt/clippy чистые; release native/WASM собраны; 44/44 модели загружаются.
  - Playwright 1.57 + обычное окно Chromium: меню/заезд/HUD/каталог/focus проверены, ошибок страницы нет. Native offscreen Vulkan — skidpad и 356a осмотрены.
  - Реверс: воспроизводимый аудит PE/EDG/JNC, оговорки о семантике флагов, инвентарь FEData/Simulation/savedata для карьер.
- Предыдущие снимки: `001-car-viewer`, `002-track-viewer`, `003-track-environment`, `004-track-topology`.
- [План 005](iterations/005-unified-driving/PLAN.md). [Следующий этап](docs/next-iteration.md).
- Задачи T01–T06 завершены, код закоммичен, тег `iteration-005`.

### 004-track-topology
- [Run 001](iterations/004-track-topology/runs/001-topology-verification/README.md): offscreen Vulkan-снимки треков и автомобиля:
  - `skidpad_topology.png`: отображение 167 кромок дорожного полотна skidpad (121 непрерывная линия) поверх геометрии без z-fighting.
  - `autobahn_topology.png`: корректная отрисовка многополосных линий и развязок автобана поверх ландшафта.
  - `car_356a_regression.png`: отсутствие визуальных и логических регрессий при отрисовке автомобилей (356A).
- Форматы `.jnc`, `.edg`, `.map` полностью исследованы и специфицированы в `local/research/004-track-topology/README.md`.
- Парсеры `nfs-formats/src/topology.rs`: валидированы все 15 трасс игры без сбоев.
- Сборка полилиний в `nfs-assets/src/track_loader.rs`: связывание последовательных сегментов границ дорожного полотна.
- Рендеринг `TopologyRenderer` в `porsche-viewer`: WGSL шейдер линий, глубинное смещение `clip_pos.z - 0.0004 * clip_pos.w`, переключатель видимости (hotkey `T`, API `set_show_topology`).
- Веб-интерфейс: автозагрузка ресурсов по дефолту из `local/game` (без необходимости выбирать файлы вручную), сохранение кнопок ручной загрузки («Папка игры», «Файлы») для переопределения с кнопкой сброса обратно к `local/game`, нативный выпадающий список `<select id="targetSelect">` для мгновенного выбора трасс/авто без стирания текста, чекбокс «3D Границы».
- **Аркадная навигация по трассе (Arcade Car Navigation)**:
  - Стилизованный спорткар Porsche (`car_mesh.rs`) со сменой цветов палитры (горячая клавиша `P`).
  - Аркадная физика автомобиля (`arcade.rs`): разгон, торможение, руление с учётом скорости, ручной тормоз с заносом (Space), расчёт передачи (1..5, R, N) и оборотов двигателя (RPM).
  - Привязка к дорожному полотну: адаптация высоты ($Y$) и наклона кузова по уклону и виражам дороги (`sample_road_elevation_fromEdges`), отскок от границ `.edg`.
  - Динамическая камера от 3-го лица с плавной интерполяцией и переключением видов (Chase / Bumper / Free, клавиша `C`).
  - Гоночный HUD в веб-версии: полупрозрачный стеклянный спидометр (км/ч), индикатор передачи, шкала тахометра (RPM) и подсказка клавиш. В нативном окне статус отображается в заголовке окна.
- Исследовательские скрипты реверса сохранены в версионируемой папке `scripts/research/`.
- Тесты: 53 unit-теста проходят (26 в assets, 21 в formats, 6 в viewer), `cargo clippy --all-targets -- -D warnings` и `cargo fmt` чистые, release native и WASM сборки собраны. Интерактивная проверка в браузере выполнена (Playwright + ручной тест).

### 003-track-environment
- [Run 001](iterations/003-track-environment/runs/001-environment-baseline/README.md): offscreen Vulkan-снимки трека `skidpad`:
  - `skidpad-overview.png`: общий вид кругового трека Skidpad с высоты с расставленными пропами (конусы слалома, стартовая стрелка) и панорамой горизонта.
  - `skidpad-cone-macro.png`: макро-вид дорожного конуса `CONE`, стоящего на поверхности асфальта, без парения и смещений.
  - `skidpad-arrow-macro.png`: крупный план стартового указателя направления `ARW1`.
  - `skidpad-driver-view.png`: вид с водительской позиции на трассу (конус створа, дорожное покрытие, строения, ограда и естественный горизонт).
- Парсинг сценариев расстановки `.scn` (`nfs-formats/src/scn.rs`): декодирование координат, ориентации 3x3 и FourCC. Сопоставление 11/11 элементов на `skidpad_st1.scn`.
- Инстанцирование динамических библиотечных пропов (`Base & 0x8000 != 0`): шаблоны пропов сохраняются в `scene.prop_articles` и отрисовываются исключительно через проход инстанцирования (`PropDraw`), предотвращая артефакт парения в начале координат (0, 0, 0).
- Загрузка и рендеринг неба: текстура горизонта `horz` из `Sky/<track>.fsh` (два вертикальных тайла 256x128) разворачивается в цилиндрическую панораму 360° с гладким затуханием в фоновый цвет земли/неба (`sky.wgsl`).
- Нулевая регрессия: все 44 автомобиля (`44/44 passed`) и все 15 трасс (`15/15 passed`) успешно загружаются и валидируются.
- Тесты: 40 unit tests pass, `cargo clippy -- -D warnings` чист, `cargo fmt` проверен, native и WASM сборки собраны.

### 002-track-viewer
- [Run 001](iterations/002-track-viewer/runs/001-track-baseline/README.md): offscreen Vulkan-снимки трека `skidpad`:
  - `skidpad-overview.png`: общий вид кругового трека, внутренней площадки и ландшафта.
  - `skidpad-road-joint.png`: макро-вид бесшовного стыка смежных блоков дорожного полотна `RD0040C` и `RD0048C` (расстояние между вершинами 0.00000).
  - `skidpad-tirewall.png`: макро-вид шинных отбойников `tir1`, каменных стен, деревьев и строений.
- [Run 002](iterations/002-track-viewer/runs/002-texture-format-and-batching/README.md): исправление 16-битных текстур FSH `0x7e` (ARGB 1555) и батчинг мешей:
  - Устранена фиолетовая пикселизация на `farmland`, `castle`, `foothills`, `industrial`, `monaco1`, `monaco2`, `monaco4`.
  - Батчинг геометрии по материалам сократил число буферов/мешей на 97% (`castle` с 11 296 до 263, `industrial` с 8 101 до 240, `farmland` с 3 836 до 184), предотвратив исчерпание ресурсов WebGPU.
  - Добавлены снимки с высоким приближением (zoom-in) и общим планом, подтверждающие отсутствие артефактов.
- Парсинг `karT` контейнеров CRP (`nfs-formats/src/track.rs`), поддержка типов примитивов 3 (треугольники), 4 (квады) и 1 (triangle strips с пропуском дегенеративных связок).
- Декодер FSH поддерживает форматы: `0x7d` (BGRA 32bpp), `0x7f` (BGR 24bpp), `0x7b` (палитровые 8bpp), `0x78` и `0x7e` (ARGB 1555 16bpp с 1-битной альфой). Все 15 трасс игры успешно валидируются и загружаются.
- Сборка мешей `nfs-assets/src/track_loader.rs`: 100% сопоставление материалов по `mt+0x28` с именами FSH-текстур. Координаты в метрах, трансформация `[x, y, -z]` с обходом CCW даёт правильные нормали дорожного полотна.
- Фильтрация библиотечных пропов по флагу `Base & 0x8000 != 0` устранила артефакт парящих объектов в центре трека.
- Нулевая регрессия: все 44 автомобиля из итерации 001 успешно загружаются (`verify_all_cars.py`: 44/44 passed).
- Тесты: 37 unit tests pass, `cargo clippy -- -D warnings` чист, release native и WASM сборки собраны.

### 001-car-viewer
- [Run010](iterations/001-car-viewer/runs/010-material-depth/README.md): все 44
  модели загрузились, 88 front/rear GPU-снимков осмотрены, без новых явных дефектов.
- Исправлены Base/style/LOD, последовательные pr-каналы, масштаб/tr/Z/winding,
  FSH-атласы, колёса и оформление специальных моделей.
- Перекрытие фар 356b исправлено signed полем материала mt+0x114 -> depth bias.
- Краска использует alpha-маску: лампы/хром сохраняют цвет.

## Приёмка и ограничения

Итерация `009-game-shell` полностью принята (тег `iteration-009`).
Следующий этап — `010-evolution-career`: полный цикл эпохи Classic, турнирное древо `nfs5.trn`, покупка/продажа подержанных авто и рынок запчастей.
См. `docs/next-iteration.md` и полную дорожную карту `docs/roadmap.md`.

## Основание проекта

Rust/wgpu/winit, shared native/WASM Scene, четыре crate в workspace (`nfs-formats`, `nfs-assets`, `nfs-game`, `porsche-viewer`).
local/game: 1742 файла / 599126359 байт, SHA-256 манифест в docs/game-manifest.json.
Среда и версии: docs/environment.md и docs/building.md; формат: docs/crp-format.md.
Git main, локальные снимки iteration-001..iteration-009; remote не настроен.
