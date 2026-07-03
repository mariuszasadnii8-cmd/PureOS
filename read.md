# PureOS — Кристаллическое неизменяемое ядро v0.4

## Что нового в v0.4

| Фича | Статус | Файл |
|------|--------|------|
| **Frame reclamation** — фреймы возвращаются при exit процесса | ✅ | `frame.rs` |
| **Приватные PML4** для каждого процесса | ✅ | `syscall.rs::spawn_initial_user` |
| **Preemptive Round-Robin** через APIC-таймер | ✅ | `apic.rs`, `idt.rs` |
| **Per-process FD tables** — полноценные open/close/dup/fcntl | ✅ | `syscall.rs` |
| **ATA PIO disk driver** — чтение/запись секторов | ✅ | `ata.rs` |
| **Block filesystem** — persistent-слой поверх диска | ✅ | `blockfs.rs` |
| **Улучшенный userspace** — расширенная C-библиотека | ✅ | `userspace/sys_c/` |
| **Graphics syscalls** (33-40) — полный набор примитивов | ✅ | `syscall.rs` |

## Что изменилось в этом апдейте (help/manuals + kernel improvement)

### Версия обновлена 0.3 → 0.4 везде
- `shell.rs`: баннер, info, version — все ссылки на v0.4
- `commands.rs`: uname показывает 0.4.0
- `fs.rs`: /etc/release, /etc/pureos-version, /etc/motd — все файлы v0.4

### Help/manuals полностью переписаны
- **`shell::cmd_help()`** — компактный список всех 30+ команд с категориями
- **`commands::cmd_help()`** — детальный help по 6 категориям (File System, System, Execution, Utilities, Network, Power)
- **`documentation.rs`** — 15 статей: Introduction, Architecture, v04, Memory, Processes, IPC, Syscalls, APIC Timer, Graphics, Barrel Graphics, Barrel Language, ATA Driver, BlockFS, Shell Commands, Installation
- **`show_command_help()`** — полный man-справочник для всех 40+ команд с usage/options/examples
- **Баннер** оболочки показывает info/man/barrel/top вместо вчерашнего минимума
- **`cmd_info`/`cmd_version`** — показывают APIC, ATA, BlockFS, PS/2, framebuffer

### Kernel improvement: real uptime + kill + process accounting + text commands
- **`PCB.switch_count`** — сколько раз процесс получал CPU (счётчик переключений)
- **`PCB.start_tick`** — тик, когда процесс был создан
- **`TICK_COUNT`** — счётчик тиков APIC-таймера, экспортирован как `get_tick_count()`
- **`uptime`** — реальное время жизни системы (часы/минуты/секунды от APIC-тиков)
- **`kill <pid>`** — реальное завершение процесса: флаг Exited + free_process_frames + очистка FD table
- **`head <file> [n]`** — показывает первые n строк реального файла
- **`tail <file> [n]`** — показывает последние n строк реального файла
- **`grep <pattern> <file>`** — ищет подстроку в файле, выводит совпадения
- **`sysmon` (top)** — uptime, загрузка памяти %, preemptive scheduler info, ticks, switch_count на процесс

### Shell improvements
- новый баннер с 4 подсказками
- `cmd_info` показывает все подсистемы (APIC, ATA, BlockFS, PS/2, barrel, graphics)
- `cmd_version` показывает архитектурные детали
- Убраны не-ASCII символы (`—` → `-`, `→` → `->`) из byte-строк
- /etc/motd и /etc/release расширены с описанием v0.4 фич

## Syscall-таблица (номер в RAX)

## Архитектура памяти

```
0x0000_0000_0000_0000 - 0x0000_0000_1000_0000  Physical Memory (Identity-mapped)
0x0000_1000_0000_0000 - 0x0000_1001_0000_0000  Ephemeral Layers (16MB × 64 processes)
0xFFFF_8000_0000_0000 - 0xFFFF_FFFF_FFFF_FFFF  Kernel Code/Data
```

**Frame allocator**: bump + free-list. Фреймы возвращаются при `exit` процесса.
**Per-process PML4**: каждый процесс имеет собственные таблицы страниц.

## Планировщик

- **APIC-таймер** (вектор 0x20) — вытесняет процесс каждые ~50ms
- **Round-Robin** — следующий runnable процесс по кругу
- **Процесс 0** (shell) не вытесняется
- `cli` → APIC init → `sti` в последовательности загрузки

## Пользовательский ленд

Си-библиотека в `userspace/sys_c/syscalls.h`:
- Все 40 syscall с inline-обёртками
- `pureos_itoa`, `pureos_strlen`, `pureos_strcmp`
- Цветовые константы
- Графические примитивы
- Файловый ввод-вывод

Примеры:
- `main.c` — демо всех возможностей
- `mandelbrot.c` — фрактальный рендеринг
- `test_proc.asm` — чистый ASM-процесс

## Дисковая подсистема

- ATA PIO (primary channel, LBA28)
- Блочная ФС: суперблок → inode → блоки данных
- Монтируется при загрузке, форматируется при отсутствии
- Синхронизируется с ramfs

## Сборка

```bash
make kernel   # ядро (x86_64-unknown-none)
make uefi     # UEFI-загрузчик
make esp      # ESP-директория
make run      # QEMU
```

## Коды ошибок

| Код | Константа | Значение |
|-----|-----------|----------|
| -1 | `ERR_INVALID_SYSCALL` | Неизвестный номер |
| -2 | `ERR_INVALID_PROCESS` | Неверный PID |
| -3 | `ERR_NO_CAPACITY` | Нет свободных слотов |
| -4 | `ERR_INVALID_POINTER` | Некорректный указатель |
| -5 | `ERR_OUT_OF_MEMORY` | Нет физической памяти |
| -11 | `-EWOULDBLOCK` | Нет данных для чтения |
| -38 | `ERR_UNSUPPORTED` | Не реализовано |
