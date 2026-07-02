# PureOS — полностью UEFI, нуль-аллок, скрипты Barrel

## Syscall-таблица (номер в RAX)

### Базовые (1-15)

| № | Имя | Параметры | Описание | Статус |
|---|-----|-----------|----------|--------|
| 1 | `memory_allocate` | `rdi=size`, `rsi=flags` | Выделить эфемерную память | ✅ |
| 2 | `memory_free` | `rdi=addr` | Освободить память (no-op) | ⚠ stub |
| 3 | `create_process` | `rdi=entry` | Создать процесс | ✅ |
| 4 | `create_thread` | `rdi=entry`, `rsi=stack` | Создать поток | ✅ |
| 5 | `yield` | — | Уступить CPU | ✅ |
| 6 | `exit` | `rdi=code` | Завершить процесс | ✅ |
| 7 | `send_ipc` | `rdi=target_pid`, `rsi=msg_ptr` | Отправить IPC | ✅ |
| 8 | `receive_ipc` | `rdi=msg_ptr` | Принять IPC | ✅ |
| 9 | `reply_ipc` | `rdi=target_pid`, `rsi=msg_ptr` | Ответить на IPC | ✅ |
| 10 | `share_memory` | `rdi=pid`, `rsi=addr`, `rdx=size` | Расшарить память | ❌ |
| 11 | `pci_device_access` | `rdi=bdf`, `rsi=offset` | Доступ к PCI | ❌ |
| 12 | `map_physical_memory` | `rdi=phys`, `rsi=size` | Отобразить MMIO | ❌ |
| 13 | `create_shared_buffer` | `rdi=size`, `rsi=flags` | Разделяемый буфер | ❌ |
| 14 | `wait_for_vblank` | — | Ожидание VBlank | ❌ |
| 15 | `exec_elf` | `rdi=data_ptr`, `rsi=size` | Загрузить ELF-процесс | ✅ |

### Файловые (16-23)

| № | Имя | Параметры | Описание | Статус |
|---|-----|-----------|----------|--------|
| 16 | `write` | `rdi=fd`, `rsi=buf`, `rdx=len` | Писать в fd | ✅ |
| 17 | `read` | `rdi=fd`, `rsi=buf`, `rdx=len` | Читать из fd | ✅ |
| 18 | `open` | `rdi=flags`, `rsi=path` | Открыть устройство | ⚠ min |
| 19 | `close` | `rdi=fd` | Закрыть дескриптор | ⚠ stub |
| 20 | `lseek` | `rdi=fd`, `rsi=offset`, `rdx=whence` | Позиция в файле | ❌ |
| 21 | `stat` | `rdi=path`, `rsi=buf` | Информация о файле | ❌ |
| 22 | `dup` | `rdi=fd` | Копировать дескриптор | ⚠ min |
| 23 | `fcntl` | `rdi=fd`, `rsi=cmd`, `rdx=arg` | Управление fd | ❌ |

### 🌟 Магические syscall (24-31) — для быстрого юзерленда

| № | Имя | Параметры | Описание |
|---|-----|-----------|----------|
| **24** | **`print`** | `rdi=buf`, `rsi=len` | Напечатать строку в терминал |
| **25** | **`println`** | `rdi=buf`, `rsi=len` | Напечатать строку + `\n` |
| **26** | **`input`** | `rdi=buf`, `rsi=maxlen` | Прочитать строку с клавиатуры (блок.) |
| **27** | **`ticks`** | — | Системный тик планировщика |
| **28** | **`cls`** | — | Очистить экран |
| **29** | **`set_cursor`** | `rdi=row`, `rsi=col` | Установить курсор |
| **30** | **`color`** | `rdi=fg`, `rsi=bg` | Установить цвета |
| **31** | **`reboot`** | — | Перезагрузка через UEFI Runtime |

Эти syscall можно вызывать прямо из ассемблера:

```asm
mov rax, 24        ; sys_print
mov rdi, msg       ; buf
mov rsi, 13        ; len
syscall
```

Или из C через обёртки в `userspace/sys_c/syscalls.h`.

## 🛢 Barrel — встроенный скриптовый язык

Интерактивный REPL: `barrel` в оболочке → `barrel>`.

### Примеры

```
print "hello world" ;
println "hello" ;

let x = 42 ;
print x ;

let name = input ;
println "hello, " name ;

if x > 10 { print "big" } else { print "small" } ;

loop { print "." ; if x == 0 { break } ; let x = x - 1 } ;

while x > 0 { print x ; let x = x - 1 } ;
```

### Синтаксис

- `print <expr> ;` — вывод
- `println <expr> ;` — вывод + `\n`
- `let <name> = <expr> ;` — переменная
- `input <name> ;` — чтение строки
- `if <cond> { ... } else { ... }`
- `loop { ... }` — бесконечный цикл
- `while <cond> { ... }`
- `break` — выход из цикла
- `// комментарий`

Выражения: числа, строки (`"..."`), переменные, `+` `-` `*` `/`, сравнения `<` `>` `==` `!=`.

## UEFI-only (без legacy)

- **Клавиатура**: UEFI Simple Text Input Protocol (вместо PS/2 0x60/0x64)
- **Debug**: UEFI ConOut (вместо COM-порта 0x3F8)
- **Фреймбуфер**: UEFI GOP (как и раньше)
- **Runtime**: ResetSystem для reboot/shutdown
- **ExitBootServices**: НЕ вызывается — UEFI протоколы доступны всё время

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
