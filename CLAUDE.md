# PureOS — Кристаллическое неизменяемое ядро (Immutable Ephemeral Kernel)

> Этот файл читается ассистентом в начале каждой сессии. Здесь — концепция,
> её отображение на реальный код, жёсткие инварианты и статус вех.
> Правь его, когда меняется архитектура, а не постфактум.

## 1. Идея одним абзацем

Ядро под быструю память (RAM/ROM, в будущем NVM/CXL). При старте оно
«замораживается» под конкретное железо в неизменяемый монолит («кристалл»).
В рантайме ядро **никогда не меняет своё состояние**: нет кучи (heap), нет
динамических глобалов, ядро себе память не выделяет. Всё изменяемое состояние
живёт в **эфемерных слоях** («ветках», как слои в Photoshop) — их создают
процессы, и они испаряются по завершении. Отсюда: нулевой износ памяти, защита
от руткитов (код ядра физически неизменен в рантайме), мгновенный старт.

Цель — реализовать это на **AMD64 (x86_64)**, работая целиком из **RAM+ROM**
(NVM/CXL — будущая веха, сейчас не требуется).

## 2. Как концепция ложится на код

| Принцип концепции | Где в коде | Как реализовано сейчас |
|---|---|---|
| Кристалл замораживается под железо | `kernel/src/main.rs` → `freeze_topology()` + `static mut TOPOLOGY` | Топология (CPU/RAM/ROM) заполняется 1 раз из `PureBootInfo`, дальше read-only по смыслу |
| Ядро не имеет кучи (Zero-Alloc) | всё ядро | `#![no_std]`, `panic=abort`, ни одного аллокатора. Всё состояние — `static mut` фиксированного размера |
| Статичное состояние | `kernel/src/syscall.rs` | `PROCESS_TABLE`, `PROCESS_PML4`, `KERNEL_STACKS`, `USER_STACKS` — фиксированные массивы на `MAX_PROCESSES = 64` |
| Эфемерные слои процессов | `kernel/src/ephemeral.rs` + `syscall.rs` + `frame.rs` | Каждому процессу — окно `EPHEMERAL_BASE(16 TiB) + i*16MiB`. `memory_allocate` берёт физфрейм из `frame::alloc_frame` и мапит его в PML4 процесса (U/S+RW) через `cpu::map_page`. Bump, **без освобождения** — слой «испаряется» на `exit` |
| Терминал (чёрный экран) | `kernel/src/terminal.rs` + `keyboard.rs` | После загрузки — scrolling-терминал на весь фреймбуфер. **UEFI Simple Text Input** вместо PS/2 |
| Встроенная оболочка | `kernel/src/shell.rs` | Команды: `help`, `clear`, `ps`, `info`, `exec`, `echo`, `demo`, `hex`, `barrel`, `reboot`, `shutdown`. Работает в процессе 0 как shell + планировщик |
| Скриптовый язык Barrel | `kernel/src/barrel.rs` | Встроенный интерпретатор: tokenizer → AST → executor. REPL через команду `barrel`. Zero-Alloc |
| UEFI-обёртки | `kernel/src/uefi.rs` | `extern "win64"` вызовы UEFI-протоколов: Simple Text Input, ConOut, Runtime Services (ResetSystem), Boot Services (Stall). ExitBootServices НЕ вызывается |
| Магические syscall (24-31) | `kernel/src/syscall.rs` | High-level: `print`, `println`, `input`, `ticks`, `cls`, `set_cursor`, `color`, `reboot` |
| Файловые syscall (16-23) | `kernel/src/syscall.rs` | `write`/`read`/`open`/`close`/`lseek`/`stat`/`dup`/`fcntl`. MVP: `write`→терминал, `read`→клавиатура |
| Физический пул RAM | `frame.rs` ← `PureBootInfo.heap_base/size` | Загрузчик резервирует 64 MiB через UEFI `AllocatePages`; ядро раздаёт занулённые фреймы bump-аллокатором |
| Неизменяемость / изоляция | `kernel/src/cpu.rs` | ring0/ring3 через GDT+TSS, вход в юзер `iretq`, обратно только `syscall`/прерывание |
| Обработка исключений CPU | `kernel/src/idt.rs` | Своя IDT (256 гейтов). Векторы 0-21 → диагносты: печатают вектор/errcode/RIP/CR2 в serial+ConOut и `hlt`. #DF/#SS/#GP/#PF на IST1. Без неё любой промах = тройная ошибка |
| Ветка = переключение контекста | `kernel/src/context.rs` → `context_switch` | Round-Robin поверх честного сохранения callee-saved + RFLAGS + RSP + CR3 |

## 3. Архитектура сборки и загрузки

- **Загрузчик** `uefi_boot/` (target `x86_64-unknown-uefi`): грузит
  `\EFI\PUREOS\KERNEL.ELF`, парсит ELF64, раскладывает `PT_LOAD`-сегменты по
  физ. адресам, инициализирует GOP-фреймбуфер, собирает `PureBootInfo` и
  прыгает в `_start(boot_info)`.
- **Ядро** `kernel/` (target `x86_64-unknown-none`): `_start` → `kernel_main`.
- **Контракт загрузчик↔ядро** — структура `PureBootInfo` (`#[repr(C)]`) и
  `BOOT_MAGIC = 0x5055_5245_4f53_0001`. Поля: kernel_base/size, framebuffer_*,
  **heap_base/heap_size** (пул фреймов). **Определена в ДВУХ местах**
  (`kernel/src/main.rs` и `uefi_boot/src/main.rs`) — при изменении правь ОБА,
  раскладка полей обязана совпадать байт-в-байт.
- **Toolchain**: Rust nightly (`rust-toolchain.toml`), нужен `rust-src`.
- **Сборка/запуск** (Makefile, SHELL = PowerShell на Windows):
  - `make kernel` / `make uefi` — сборка компонентов.
  - `make iso` — собрать ESP + загрузочный ISO (нужен `xorriso`).
  - `make run` — QEMU (`qemu-system-x86_64`, нужен `OVMF_CODE.fd`, `-m 512M`).
  - `make check-tools` — проверить наличие xorriso/qemu.

## 4. Запуск ядра (последовательность `kernel_main`)

1. `freeze_topology(boot_info)` — заморозить топологию.
2. `init_frame_pool(boot_info)` — пул фреймов от загрузчика.
0. `cli` + `mask_legacy_pic()` — ПЕРВЫМ делом глушим прерывания (UEFI оставляет
   их вкл. и держит свою IDT под свою GDT; после нашего `lgdt` тик таймера =
   тройная ошибка). Дальше весь рантайм работает без аппаратных прерываний.
1. `freeze_topology(boot_info)` — заморозить топологию.
2. `init_frame_pool(boot_info)` — пул фреймов от загрузчика.
3. `framebuffer::init(...)` + `console::draw_boot_screen()` — заставка и лог.
4. `cpu::init_gdt()` — GDT + TSS + перезагрузка сегментов + `ltr`.
5. `idt::init()` + `idt::load()` — своя IDT с диагностами исключений.
6. `syscall::init_process_manager()` — процесс 0 = резидентный планировщик/idle (ring0).
7. `syscall::init_syscall_msrs()` — `EFER.SCE`, `STAR/LSTAR/FMASK`, `KERNEL_GS_BASE`.
8. `keyboard::init()` + `terminal::init()` — UEFI ввод/вывод.
9. `spawn_initial_user(...)` — демо-процесс ring3 (дымовой тест моста); его код/
   стек помечаются доступными из ring3 (`make_user_accessible`).
10. `shell::run()` — процесс 0: оболочка + Round-Robin (заменил `run_scheduler`).

## 5. ЖЁСТКИЕ ИНВАРИАНТЫ (легко сломать — читай перед правкой ядра)

- **Zero-heap / Zero-alloc**: не вводить аллокатор, `Vec`, `Box`, `alloc` в
  ядре. Только `static mut` фиксированного размера. Куча есть ТОЛЬКО в
  загрузчике (`uefi_boot`, там `extern crate alloc` — это ок, UEFI-стадия).
- **Ядро отображено одинаково во всех PML4** процессов. Иначе `context_switch`
  (смена CR3) и побайтовый IPC-копир (`copy_user_message`) взорвутся. Любой
  новый маппинг ядра должен попадать во все адресные пространства.
- **Identity-map физпамяти**: `frame`/`map_page` считают phys == указатель.
  Держится, пока не вызван `ExitBootServices` (мы его НЕ вызываем) — работаем на
  таблицах OVMF, которые 1:1 отображают низкую физпамять (включая пул фреймов).
  Если введёшь `ExitBootServices`/свои таблицы — сначала обеспечь identity-map
  пула, иначе `alloc_frame` (зануление) и запись в таблицы дадут #PF.
- **Эфемерное окно (16 TiB, `EPHEMERAL_BASE`)** выбрано вне identity-map, чтобы
  `map_page` не пересекал существующие записи. Не опускай его в низкую память.
- **Раскладка `PerCpu` фиксирована**: трамплин `syscall_entry` читает `gs:[0]`
  (kernel_rsp) и `gs:[8]` (user_rsp_scratch). Не переставлять поля.
- **Раскладка GDT привязана к `syscall/sysret`**: селекторы `0x08/0x10/0x18|3/
  0x20|3` и `STAR` должны согласовываться (см. шапку `cpu.rs`).
- **Кадр начального стека** (`prepare_initial_stack`) должен совпадать с
  эпилогом `context_switch` (`popfq; pop r15..rbx; ret`). Меняешь одно — правь второе.
- **`PureBootInfo` дублируется** в ядре и загрузчике (см. §3). Поля `system_table` и `con_in` передаются от UEFI загрузчика в ядро.
- **Прерывания выключены весь рантайм** (`cli` в начале `kernel_main`, IF=0 в
  кадре ring3 и в `prepare_initial_stack`, `FMASK` гасит IF на `syscall`).
  Планировщик кооперативный, клавиатура опрашивается. НЕ включай `sti`, не
  подняв сперва свой контроллёр прерываний (APIC) и обработчики в `idt.rs` —
  иначе UEFI-таймер/IRQ уронят машину. Ставишь таймерное вытеснение (веха №4) —
  сначала настрой APIC, потом IF=1.
- **IDT грузится ПОСЛЕ GDT** (`idt.rs` использует селектор `KERNEL_CS=0x08`).
  Критичные исключения (#DF/#SS/#GP/#PF) уходят на `IST1`, который заполняется в
  `cpu::init_gdt` (TSS.ist1). Порядок init_gdt → idt::load обязателен.

## 6. Карта syscall (номер в RAX)

1 alloc · 2 free · 3 create_process · 4 create_thread · 5 yield · 6 exit ·
7 send_ipc · 8 receive_ipc · 9 reply_ipc · 10 share_memory* · 11 pci* ·
12 map_phys* · 13 shared_buffer* · 14 wait_vblank* · **15 exec_elf** ·
16 write · 17 read · 18 open · 19 close · 20 lseek* · 21 stat* · 22 dup · 23 fcntl* ·
**24 print · 25 println · 26 input · 27 ticks · 28 cls · 29 set_cursor · 30 color · 31 reboot**
(`*` = `ERR_UNSUPPORTED`, ждут frame-allocator/драйверов).
Коды ошибок: `-1` invalid syscall, `-2` invalid proc, `-3` no capacity,
`-4` invalid ptr, `-5` OOM, `-38` unsupported.

**Syscall 15 (exec_elf)**: загружает статический PIE ELF64-бинарник как новый
userspace-процесс. Параметры: `rdi=data_ptr` (указатель на ELF в памяти
отправителя), `rsi=size`. Возвращает PID или ошибку. Сегменты PT_LOAD
отображаются в приватную PML4 процесса; стек 64 KiB — сразу за кодом.
ELF-загрузчик в `kernel/src/elf.rs` — это FFI-мост для любых языков,
компилирующих статический x86_64 ELF (Rust no_std, C, Zig, Kotlin/Native, Go).

**Syscalls 24-31 (магические)**: высокоуровневые обёртки для быстрого юзерленда.
`print`/`println` пишут напрямую в терминал, `input` — блокирующий ввод строки,
`reboot` дёргает UEFI ResetSystem. Полный список — в `read.md`.

IPC — синхронное рандеву без буфера в ядре: send↔receive↔reply, копирование
напрямую между адресными пространствами через временную смену CR3.

**Barrel**: встроенный скриптовый язык. REPL через команду `barrel` в shell.
Интерпретатор в `kernel/src/barrel.rs` — zero-alloc, статические буферы.
Поддерживает: переменные, арифметику, сравнения, `print`/`println`/`input`,
`if`/`else`, `loop`, `while`, `break`.

**UEFI-only**: ядро использует UEFI Simple Text Input вместо PS/2, UEFI ConOut
вместо COM-порта, UEFI Runtime Services для reboot/shutdown. ExitBootServices
НЕ вызывается — UEFI протоколы доступны весь рантайм.

## 7. Статус и следующие вехи

**Сделано:** UEFI-загрузка + ELF, заморозка топологии, GDT/TSS, мост
ring0↔ring3 (`syscall/sysret`), Round-Robin с честным переключением контекста,
синхронный IPC-рандеву, **физический frame-allocator (bump) + честное
отображение эфемерных слоёв в PML4 процесса** (`frame.rs`, `cpu::map_page`,
`memory_allocate`). ⚡ Фреймбуфер + консоль: загрузочный экран с кристаллом,
анимация в планировщике. **ELF-загрузчик** (syscall 15, `elf.rs`) — FFI-мост
для C/Kotlin/Zig/Rust-бинарников. **Терминал** (`terminal.rs`) — чёрный экран,
прокрутка, палитра. **UEFI Simple Text Input** (`keyboard.rs`) — клавиатура
через UEFI-протокол, без PS/2. **UEFI-обёртки** (`uefi.rs`) — ConOut для debug,
Runtime Services для reboot/shutdown. **Оболочка** (`shell.rs`) — встроенные
команды + планировщик + Barrel REPL. **Файловые syscalls** (16-23) —
write/read/open/close/dup. **Магические syscall** (24-31) — print/println/input/
ticks/cls/set_cursor/color/reboot. **Barrel** (`barrel.rs`) — встроенный
скриптовый язык: переменные, арифметика, if/else, loop/while/break.
**IDT + диагностика исключений** (`idt.rs`) и **устранён reboot-loop**: UEFI
оставлял прерывания включёнными со своей IDT — после подмены нашей GDT первый же
тик таймера давал тройную ошибку (немой ребут). Теперь `cli`+маска PIC в начале
`kernel_main`, своя IDT, вход в ring3 с IF=0. Сборка ядра и загрузчика — **без
предупреждений**.

⚠ Правки собраны (`cargo build` ядра и uefi_boot — чисто), но в QEMU ещё НЕ
прогонялись (на машине разработки нет qemu/xorriso/OVMF) — нужен финальный
runtime-smoke-test. Теперь при любом исключении CPU вместо ребута появится
диагностика (вектор/errcode/RIP/CR2) в serial и на экране.

**Ключевые вехи (в порядке приоритета):**
1. **Runtime-smoke-test** — прогнать `make run` на машине с QEMU/OVMF. Ожидается:
   загрузка → boot-экран → лог `[CPU]/[IDT]/[SYS]` → баннер оболочки `pureos$`,
   БЕЗ ребут-цикла. Если что-то падает — теперь виден вектор исключения.
2. **Рекламация фреймов на `exit`** — сейчас bump без возврата; слой процесса
   при выходе не отдаёт фреймы в пул. Нужен per-process учёт (или отдельный пул
   на слой) для реального «испарения».
3. **Приватные таблицы для `spawn_initial_user`** — начальные процессы всё ещё
   делят boot cr3 (и `make_user_accessible` грубо помечает страницы ядра U/S —
   дыра в изоляции, временно ради демо). `create_process` уже клонирует PML4;
   свести оба пути к приватным таблицам. Разблокирует `share_memory`/`map_phys`.
4. **Вытесняющий таймер (APIC)** — планировщик кооперативный, прерывания
   выключены. Для вытеснения: настроить LAPIC-таймер, добавить его вектор в
   `idt.rs`, только потом `sti`/IF=1 (см. инвариант о прерываниях в §5).
5. **ROM/NVM-режим** — прописать `rom_base/rom_size` (сейчас = 0), исполнять
   кристалл прямо из ROM/NVM.
6. **Реальный userspace** (`userspace/`) вместо `user_demo`.

## 8. Правила работы с этим проектом

- Комментарии в коде — на русском, техтермины латиницей. Держи стиль.
- Всё в ядре — `unsafe`/низкоуровневое; проверяй инварианты §5 перед правкой.
- Отлаживать через `make run` в QEMU (Windows-хост, PowerShell в Makefile).
- Не коммить и не пушь без явной просьбы пользователя.
