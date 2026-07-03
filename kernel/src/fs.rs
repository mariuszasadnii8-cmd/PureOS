//! Кристаллическая in-RAM файловая система PureOS (ramfs).
//!
//! Zero-Alloc: всё состояние — статические массивы фиксированного размера, ни
//! одной динамической аллокации. По духу эфемерного ядра: дерево живёт в RAM,
//! испаряется при перезагрузке (persistence на диск — будущая веха).
//!
//! Модель: массив узлов `NODES[MAX_NODES]`. Каждый узел — либо каталог (`Dir`),
//! либо файл (`File`), либо свободный слот (`Free`). Дети каталога находятся
//! линейным сканом по полю `parent` (n маленькое, поэтому O(n) достаточно).
//! Данные файлов лежат в общем байтовом пуле `DATA` c bump-выдачей; перезапись,
//! не влезающая в текущую ёмкость, берёт новый участок (старый «протекает» —
//! это согласуется с эфемерной философией; полноценная рекламация — позже).
//!
//! Пути: абсолютные (`/a/b`) и относительные (от CWD). Поддержаны `.` и `..`.

const MAX_NODES: usize = 256;
const NAME_MAX: usize = 28;
const DATA_POOL: usize = 256 * 1024;

pub const ROOT: u16 = 0;

#[derive(Copy, Clone, PartialEq)]
pub enum Kind {
    Free,
    Dir,
    File,
}

#[derive(Copy, Clone)]
pub struct Node {
    name: [u8; NAME_MAX],
    name_len: u8,
    pub kind: Kind,
    parent: u16,
    data_off: u32,
    data_len: u32,
    data_cap: u32,
}

impl Node {
    const fn empty() -> Self {
        Self {
            name: [0; NAME_MAX],
            name_len: 0,
            kind: Kind::Free,
            parent: 0,
            data_off: 0,
            data_len: 0,
            data_cap: 0,
        }
    }
    pub fn name(&self) -> &[u8] {
        &self.name[..self.name_len as usize]
    }
}

static mut NODES: [Node; MAX_NODES] = [Node::empty(); MAX_NODES];
static mut DATA: [u8; DATA_POOL] = [0; DATA_POOL];
static mut DATA_NEXT: u32 = 0;
static mut CWD: u16 = ROOT;
static mut READY: bool = false;

/// Инициализировать ФС: корень + базовое дерево (FHS-подобное).
pub unsafe fn init() {
    if READY {
        return;
    }
    for n in NODES.iter_mut() {
        *n = Node::empty();
    }
    DATA_NEXT = 0;
    // Корень указывает parent на самого себя.
    NODES[0] = Node {
        name: name_buf(b"/"),
        name_len: 1,
        kind: Kind::Dir,
        parent: ROOT,
        data_off: 0,
        data_len: 0,
        data_cap: 0,
    };
    CWD = ROOT;
    READY = true;

    // Базовые каталоги.
    let _ = mkdir(ROOT, b"bin");
    let _ = mkdir(ROOT, b"dev");
    let _ = mkdir(ROOT, b"etc");
    let _ = mkdir(ROOT, b"home");
    let _ = mkdir(ROOT, b"tmp");

    // Приветственный файл.
    if let Some(etc) = find_child(ROOT, b"etc") {
        if let Some(motd) = create_file(etc, b"motd") {
            let _ = write(motd, b"Welcome to PureOS Crystal Kernel v0.4\n\
====================================\n\
  Preemptive round-robin via APIC timer\n\
  ATA PIO disk + block filesystem\n\
  Per-process FD tables, private PML4\n\
  Frame reclamation on exit\n\
  Barrel scripting + native compiler\n\
  Graphics primitives (40 syscalls)\n\
Type 'help' for commands.\n");
        }
        if let Some(rel) = create_file(etc, b"release") {
            let _ = write(rel, b"PureOS release 0.4 (crystal)\n\
Kernel: pureos_kernel 0.4.0\n\
Arch: x86_64 AMD64\n\
Firmware: UEFI\n\
Built: 2026-07\n");
        }
        if let Some(ver) = create_file(etc, b"pureos-version") {
            let _ = write(ver, b"0.4.0\n");
        }
    }
    if let Some(home) = find_child(ROOT, b"home") {
        let _ = mkdir(home, b"user");
    }
}

/// Получить статистику пула данных ФС.
pub fn data_pool_stats() -> (u32, u32) {
    unsafe { (DATA_POOL as u32, DATA_NEXT) }
}

pub fn is_ready() -> bool {
    unsafe { READY }
}

fn name_buf(s: &[u8]) -> [u8; NAME_MAX] {
    let mut b = [0u8; NAME_MAX];
    let n = s.len().min(NAME_MAX);
    let mut i = 0;
    while i < n {
        b[i] = s[i];
        i += 1;
    }
    b
}

fn names_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut i = 0;
    while i < a.len() {
        if a[i] != b[i] {
            return false;
        }
        i += 1;
    }
    true
}

// ---------------------------------------------------------------------------
// Базовые операции над узлами
// ---------------------------------------------------------------------------

unsafe fn alloc_node() -> Option<u16> {
    for i in 1..MAX_NODES {
        if NODES[i].kind == Kind::Free {
            return Some(i as u16);
        }
    }
    None
}

/// Найти прямого ребёнка каталога `dir` по имени.
pub unsafe fn find_child(dir: u16, name: &[u8]) -> Option<u16> {
    for i in 0..MAX_NODES {
        let n = &NODES[i];
        if n.kind != Kind::Free && n.parent == dir && i as u16 != dir && names_eq(n.name(), name) {
            return Some(i as u16);
        }
    }
    None
}

/// Создать подкаталог. Возвращает индекс узла или None (нет слота/дубликат/имя).
pub unsafe fn mkdir(dir: u16, name: &[u8]) -> Option<u16> {
    if name.is_empty() || name.len() > NAME_MAX || NODES[dir as usize].kind != Kind::Dir {
        return None;
    }
    if find_child(dir, name).is_some() {
        return None;
    }
    let idx = alloc_node()?;
    NODES[idx as usize] = Node {
        name: name_buf(name),
        name_len: name.len() as u8,
        kind: Kind::Dir,
        parent: dir,
        data_off: 0,
        data_len: 0,
        data_cap: 0,
    };
    Some(idx)
}

/// Создать пустой файл (или вернуть существующий, если это файл).
pub unsafe fn create_file(dir: u16, name: &[u8]) -> Option<u16> {
    if name.is_empty() || name.len() > NAME_MAX || NODES[dir as usize].kind != Kind::Dir {
        return None;
    }
    if let Some(existing) = find_child(dir, name) {
        return if NODES[existing as usize].kind == Kind::File {
            Some(existing)
        } else {
            None
        };
    }
    let idx = alloc_node()?;
    NODES[idx as usize] = Node {
        name: name_buf(name),
        name_len: name.len() as u8,
        kind: Kind::File,
        parent: dir,
        data_off: 0,
        data_len: 0,
        data_cap: 0,
    };
    Some(idx)
}

/// Записать данные в файл, заменив содержимое. true при успехе.
pub unsafe fn write(node: u16, data: &[u8]) -> bool {
    let n = &mut NODES[node as usize];
    if n.kind != Kind::File {
        return false;
    }
    let len = data.len() as u32;
    // Влезает в текущую ёмкость — перезаписываем на месте.
    if len <= n.data_cap {
        let off = n.data_off as usize;
        for i in 0..data.len() {
            DATA[off + i] = data[i];
        }
        n.data_len = len;
        return true;
    }
    // Нужен новый участок пула.
    let off = DATA_NEXT;
    if off as usize + data.len() > DATA_POOL {
        return false; // пул исчерпан
    }
    for i in 0..data.len() {
        DATA[off as usize + i] = data[i];
    }
    DATA_NEXT = off + len;
    n.data_off = off;
    n.data_len = len;
    n.data_cap = len;
    true
}

/// Дописать данные в конец файла. true при успехе.
pub unsafe fn append(node: u16, data: &[u8]) -> bool {
    let cur = read(node);
    let cur_len = cur.len();
    // Скопировать текущее во временную область невозможно (zero-alloc): вместо
    // этого перекладываем через прямую сборку в пуле. Простой путь — новый блок.
    let total = cur_len + data.len();
    let off = DATA_NEXT;
    if off as usize + total > DATA_POOL {
        return false;
    }
    // Скопировать старое (оно лежит по старому смещению) затем новое.
    let n = NODES[node as usize];
    if n.kind != Kind::File {
        return false;
    }
    let old_off = n.data_off as usize;
    for i in 0..cur_len {
        DATA[off as usize + i] = DATA[old_off + i];
    }
    for i in 0..data.len() {
        DATA[off as usize + cur_len + i] = data[i];
    }
    DATA_NEXT = off + total as u32;
    let nm = &mut NODES[node as usize];
    nm.data_off = off;
    nm.data_len = total as u32;
    nm.data_cap = total as u32;
    true
}

/// Прочитать содержимое файла как срез.
pub unsafe fn read(node: u16) -> &'static [u8] {
    let n = &NODES[node as usize];
    if n.kind != Kind::File || n.data_len == 0 {
        return &[];
    }
    core::slice::from_raw_parts(
        core::ptr::addr_of!(DATA[n.data_off as usize]),
        n.data_len as usize,
    )
}

/// Удалить узел (файл или пустой каталог). true при успехе.
pub unsafe fn unlink(node: u16) -> bool {
    if node == ROOT {
        return false;
    }
    let kind = NODES[node as usize].kind;
    if kind == Kind::Free {
        return false;
    }
    // Каталог удаляется только пустым.
    if kind == Kind::Dir {
        for i in 0..MAX_NODES {
            if NODES[i].kind != Kind::Free && NODES[i].parent == node && i as u16 != node {
                return false; // не пуст
            }
        }
    }
    NODES[node as usize] = Node::empty();
    true
}

pub unsafe fn kind(node: u16) -> Kind {
    NODES[node as usize].kind
}

pub unsafe fn node_name(node: u16) -> &'static [u8] {
    let n = &NODES[node as usize];
    core::slice::from_raw_parts(core::ptr::addr_of!(n.name[0]), n.name_len as usize)
}

pub unsafe fn size_of(node: u16) -> u32 {
    NODES[node as usize].data_len
}

// ---------------------------------------------------------------------------
// CWD и разрешение путей
// ---------------------------------------------------------------------------

pub fn cwd() -> u16 {
    unsafe { CWD }
}

pub unsafe fn set_cwd(node: u16) {
    if NODES[node as usize].kind == Kind::Dir {
        CWD = node;
    }
}

/// Разрешить путь (абсолютный или относительный CWD) в индекс узла.
pub unsafe fn resolve(path: &[u8]) -> Option<u16> {
    resolve_from(CWD, path)
}

pub unsafe fn resolve_from(start: u16, path: &[u8]) -> Option<u16> {
    let mut cur = if !path.is_empty() && path[0] == b'/' {
        ROOT
    } else {
        start
    };
    let mut i = 0;
    while i < path.len() {
        // Пропустить разделители.
        while i < path.len() && path[i] == b'/' {
            i += 1;
        }
        let comp_start = i;
        while i < path.len() && path[i] != b'/' {
            i += 1;
        }
        if i == comp_start {
            break; // конец
        }
        let comp = &path[comp_start..i];
        if names_eq(comp, b".") {
            continue;
        }
        if names_eq(comp, b"..") {
            cur = NODES[cur as usize].parent;
            continue;
        }
        cur = find_child(cur, comp)?;
    }
    Some(cur)
}

/// Разделить путь на (родительский каталог, имя последнего компонента).
/// Возвращает None, если родитель не существует/не каталог.
pub unsafe fn resolve_parent(path: &[u8]) -> Option<(u16, &[u8])> {
    // Найти позицию последнего '/'.
    let mut last_slash: isize = -1;
    for (idx, &c) in path.iter().enumerate() {
        if c == b'/' {
            last_slash = idx as isize;
        }
    }
    if last_slash < 0 {
        // Нет '/': родитель — CWD, имя — весь путь.
        return Some((CWD, path));
    }
    let ls = last_slash as usize;
    let leaf = &path[ls + 1..];
    if leaf.is_empty() {
        return None;
    }
    let parent = if ls == 0 {
        ROOT
    } else {
        resolve(&path[..ls])?
    };
    if NODES[parent as usize].kind != Kind::Dir {
        return None;
    }
    Some((parent, leaf))
}

/// Записать абсолютный путь узла в буфер, вернуть длину.
pub unsafe fn path_of(node: u16, out: &mut [u8]) -> usize {
    if node == ROOT {
        if !out.is_empty() {
            out[0] = b'/';
            return 1;
        }
        return 0;
    }
    // Собрать цепочку до корня (индексы), затем развернуть.
    let mut chain = [0u16; 32];
    let mut depth = 0;
    let mut cur = node;
    while cur != ROOT && depth < chain.len() {
        chain[depth] = cur;
        depth += 1;
        cur = NODES[cur as usize].parent;
    }
    let mut pos = 0;
    let mut d = depth;
    while d > 0 {
        d -= 1;
        if pos < out.len() {
            out[pos] = b'/';
            pos += 1;
        }
        let nm = NODES[chain[d] as usize];
        for k in 0..nm.name_len as usize {
            if pos < out.len() {
                out[pos] = nm.name[k];
                pos += 1;
            }
        }
    }
    pos
}

/// Итерировать детей каталога, вызывая `f(index)` для каждого.
pub unsafe fn for_each_child(dir: u16, mut f: impl FnMut(u16)) {
    if NODES[dir as usize].kind != Kind::Dir {
        return;
    }
    for i in 0..MAX_NODES {
        if NODES[i].kind != Kind::Free && NODES[i].parent == dir && i as u16 != dir {
            f(i as u16);
        }
    }
}
