//! Barrel Crystal Compiler (barrelc) — компилятор Barrel в нативный x86_64.
//!
//! В отличие от интерпретатора (`barrel.rs`), который обходит AST в ring0, этот
//! модуль ПОРОЖДАЕТ машинный код и запускает его как настоящий userspace-процесс
//! в ring3 через существующий ELF-мост (`elf::exec`, syscall 15). Это и есть
//! «компилятор под машину»: Barrel-исходник → x86_64 → ELF64 → ring3.
//!
//! Zero-Alloc: весь вывод собирается в статический буфер `ELF_BUF`.
//!
//! Пайплайн `compile_and_run`:
//!   1. однопроходный рекурсивный спуск: лексер + кодоген сразу в машкод;
//!   2. код оборачивается в минимальный ELF64 (ET_EXEC, один PT_LOAD, RWX);
//!   3. `elf::exec` раскладывает сегмент в приватную PML4 процесса (ring3);
//!   4. `syscall::run_slot` переключается на процесс; тот печатает через
//!      syscall 32 (print_num) и завершается syscall 6 (exit) → возврат в shell.
//!
//! Код полностью позиционно-независим: переходы — rel32, локали — [rbp-off],
//! вывод — syscall с непосредственными номерами. Релокации не нужны.
//!
//! Поддерживаемое подмножество Barrel:
//!   let <name> = <expr>;      print <expr>;   println <expr>;
//!   if <expr> { ... } [else { ... }]          while <expr> { ... }
//!   loop { ... }              break;
//!   выражения: числа (u64), переменные, ( ), унарный -, + - * /, < > == !=

use crate::elf;
use crate::syscall;

// Виртуальный адрес загрузки скомпилированной программы. Взят высоко (32 TiB),
// вне identity-map и эфемерного окна (16 TiB), чтобы `map_page` строил свежие
// таблицы и не натыкался на huge-страницы загрузчика. Каноничный, page-aligned.
const LOAD_VADDR: u64 = 0x0000_2000_0000_0000;

const HDR_SIZE: usize = 64;   // Elf64 header
const PHDR_SIZE: usize = 56;  // один program header
const CODE_OFF: usize = HDR_SIZE + PHDR_SIZE; // 120 — начало машкода в файле

const ELF_BUF_SIZE: usize = 4096;
static mut ELF_BUF: [u8; ELF_BUF_SIZE] = [0; ELF_BUF_SIZE];

const MAX_LOCALS: usize = 32;
const FRAME_BYTES: u32 = (MAX_LOCALS as u32) * 8; // 256, кратно 16
const MAX_BREAKS: usize = 32;

// Коды ошибок компилятора (возвращаются отрицательными наружу).
const E_PARSE: i64 = -100;
const E_OVERFLOW: i64 = -101;
const E_TOOMANY_VARS: i64 = -102;

// ===================================================================
// Токены
// ===================================================================

#[derive(Copy, Clone, PartialEq)]
enum Tk {
    Eof,
    Num,
    Ident,
    Print,
    Println,
    Let,
    If,
    Else,
    While,
    Loop,
    Break,
    Eq,
    EqEq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,
    Plus,
    Minus,
    Star,
    Slash,
    LParen,
    RParen,
    LBrace,
    RBrace,
    Semi,
}

struct Comp<'a> {
    src: &'a [u8],
    pos: usize,
    // Текущий токен.
    tok: Tk,
    num: u64,
    name_off: usize,
    name_len: usize,
    // Вывод машкода: ELF_BUF[CODE_OFF..], clen — длина кода.
    clen: usize,
    // Таблица локальных переменных (имена как срезы исходника).
    vars: [(usize, usize); MAX_LOCALS],
    nvars: usize,
    // Незапатченные сайты `break` текущих циклов.
    breaks: [usize; MAX_BREAKS],
    nbreaks: usize,
    err: i64, // 0 = ок
}

// ===================================================================
// Лексер
// ===================================================================

impl<'a> Comp<'a> {
    fn peek_ch(&self) -> u8 {
        if self.pos >= self.src.len() { 0 } else { self.src[self.pos] }
    }
    fn peek2(&self) -> u8 {
        if self.pos + 1 >= self.src.len() { 0 } else { self.src[self.pos + 1] }
    }

    fn skip_ws(&mut self) {
        loop {
            let c = self.peek_ch();
            if c == b'/' && self.peek2() == b'/' {
                while self.peek_ch() != b'\n' && self.peek_ch() != 0 { self.pos += 1; }
            } else if c == b' ' || c == b'\t' || c == b'\n' || c == b'\r' {
                self.pos += 1;
            } else {
                break;
            }
        }
    }

    /// Считать следующий токен в self.tok.
    fn next(&mut self) {
        self.skip_ws();
        let c = self.peek_ch();
        if c == 0 {
            self.tok = Tk::Eof;
            return;
        }
        match c {
            b'0'..=b'9' => {
                let mut n: u64 = 0;
                while self.peek_ch().is_ascii_digit() {
                    n = n.wrapping_mul(10).wrapping_add((self.peek_ch() - b'0') as u64);
                    self.pos += 1;
                }
                self.num = n;
                self.tok = Tk::Num;
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = self.pos;
                while self.peek_ch().is_ascii_alphanumeric() || self.peek_ch() == b'_' {
                    self.pos += 1;
                }
                let name = &self.src[start..self.pos];
                self.name_off = start;
                self.name_len = self.pos - start;
                self.tok = match name {
                    b"print" => Tk::Print,
                    b"println" => Tk::Println,
                    b"let" => Tk::Let,
                    b"if" => Tk::If,
                    b"else" => Tk::Else,
                    b"while" => Tk::While,
                    b"loop" => Tk::Loop,
                    b"break" => Tk::Break,
                    _ => Tk::Ident,
                };
            }
            b'=' => { self.pos += 1; if self.peek_ch() == b'=' { self.pos += 1; self.tok = Tk::EqEq; } else { self.tok = Tk::Eq; } }
            b'!' => { self.pos += 1; if self.peek_ch() == b'=' { self.pos += 1; self.tok = Tk::Ne; } else { self.fail(E_PARSE); } }
            b'<' => { self.pos += 1; if self.peek_ch() == b'=' { self.pos += 1; self.tok = Tk::Le; } else { self.tok = Tk::Lt; } }
            b'>' => { self.pos += 1; if self.peek_ch() == b'=' { self.pos += 1; self.tok = Tk::Ge; } else { self.tok = Tk::Gt; } }
            b'+' => { self.pos += 1; self.tok = Tk::Plus; }
            b'-' => { self.pos += 1; self.tok = Tk::Minus; }
            b'*' => { self.pos += 1; self.tok = Tk::Star; }
            b'/' => { self.pos += 1; self.tok = Tk::Slash; }
            b'(' => { self.pos += 1; self.tok = Tk::LParen; }
            b')' => { self.pos += 1; self.tok = Tk::RParen; }
            b'{' => { self.pos += 1; self.tok = Tk::LBrace; }
            b'}' => { self.pos += 1; self.tok = Tk::RBrace; }
            b';' => { self.pos += 1; self.tok = Tk::Semi; }
            _ => { self.pos += 1; self.fail(E_PARSE); }
        }
    }

    fn eat(&mut self, t: Tk) -> bool {
        if self.tok == t { self.next(); true } else { self.fail(E_PARSE); false }
    }

    fn fail(&mut self, code: i64) {
        if self.err == 0 { self.err = code; }
    }

    // ---------------------------------------------------------------
    // Переменные -> слоты кадра. Смещение слота i от rbp: -((i+1)*8).
    // ---------------------------------------------------------------
    fn var_slot(&mut self, off: usize, len: usize) -> usize {
        for i in 0..self.nvars {
            let (o, l) = self.vars[i];
            if l == len && self.src[o..o + l] == self.src[off..off + len] {
                return i;
            }
        }
        if self.nvars >= MAX_LOCALS {
            self.fail(E_TOOMANY_VARS);
            return 0;
        }
        let idx = self.nvars;
        self.vars[idx] = (off, len);
        self.nvars += 1;
        idx
    }

    fn slot_disp(slot: usize) -> i32 {
        -(((slot + 1) * 8) as i32)
    }

    // ===================================================================
    // Эмиттер машкода
    // ===================================================================

    fn emit(&mut self, b: u8) {
        let idx = CODE_OFF + self.clen;
        if idx >= ELF_BUF_SIZE {
            self.fail(E_OVERFLOW);
            return;
        }
        unsafe { ELF_BUF[idx] = b; }
        self.clen += 1;
    }

    fn emit_bytes(&mut self, bs: &[u8]) {
        for &b in bs { self.emit(b); }
    }

    fn emit_u32(&mut self, v: u32) {
        self.emit_bytes(&v.to_le_bytes());
    }

    fn emit_u64(&mut self, v: u64) {
        self.emit_bytes(&v.to_le_bytes());
    }

    /// Записать rel32 (little-endian) по ранее зарезервированному сайту.
    fn patch32(&mut self, site: usize, rel: i32) {
        let bytes = rel.to_le_bytes();
        for k in 0..4 {
            let idx = CODE_OFF + site + k;
            if idx < ELF_BUF_SIZE {
                unsafe { ELF_BUF[idx] = bytes[k]; }
            }
        }
    }

    // --- инструкции-примитивы (результат выражения всегда в rax) ---

    fn mov_rax_imm(&mut self, imm: u64) {
        self.emit_bytes(&[0x48, 0xB8]); // movabs rax, imm64
        self.emit_u64(imm);
    }
    fn push_rax(&mut self) { self.emit(0x50); }
    fn pop_rax(&mut self) { self.emit(0x58); }
    fn mov_rcx_rax(&mut self) { self.emit_bytes(&[0x48, 0x89, 0xC1]); }

    fn load_local(&mut self, slot: usize) {
        // mov rax, [rbp + disp32]
        self.emit_bytes(&[0x48, 0x8B, 0x85]);
        self.emit_u32(Self::slot_disp(slot) as u32);
    }
    fn store_local(&mut self, slot: usize) {
        // mov [rbp + disp32], rax
        self.emit_bytes(&[0x48, 0x89, 0x85]);
        self.emit_u32(Self::slot_disp(slot) as u32);
    }

    // Бинарные операции над rax (левый) и rcx (правый).
    fn op_add(&mut self) { self.emit_bytes(&[0x48, 0x01, 0xC8]); } // add rax, rcx
    fn op_sub(&mut self) { self.emit_bytes(&[0x48, 0x29, 0xC8]); } // sub rax, rcx
    fn op_mul(&mut self) { self.emit_bytes(&[0x48, 0x0F, 0xAF, 0xC1]); } // imul rax, rcx
    fn op_div(&mut self) {
        // Деление на 0 -> результат 0 (иначе #DE уронил бы ring3-процесс).
        // test rcx, rcx; je skip; xor edx,edx; div rcx; jmp done; skip: xor eax,eax; done:
        self.emit_bytes(&[0x48, 0x85, 0xC9]);       // test rcx, rcx
        self.emit_bytes(&[0x0F, 0x84]);             // je rel32
        let site_skip = self.clen; self.emit_u32(0);
        self.emit_bytes(&[0x31, 0xD2]);             // xor edx, edx
        self.emit_bytes(&[0x48, 0xF7, 0xF1]);       // div rcx
        self.emit_bytes(&[0xE9]);                   // jmp rel32
        let site_done = self.clen; self.emit_u32(0);
        let skip_target = self.clen;
        self.emit_bytes(&[0x31, 0xC0]);             // xor eax, eax
        let done_target = self.clen;
        self.patch32(site_skip, (skip_target as i32) - (site_skip as i32 + 4));
        self.patch32(site_done, (done_target as i32) - (site_done as i32 + 4));
    }
    fn op_cmp_set(&mut self, set2: u8) {
        // cmp rax, rcx; set%cc al; movzx rax, al
        self.emit_bytes(&[0x48, 0x39, 0xC8]);        // cmp rax, rcx
        self.emit_bytes(&[0x0F, set2, 0xC0]);        // setcc al
        self.emit_bytes(&[0x48, 0x0F, 0xB6, 0xC0]);  // movzx rax, al
    }
    fn op_neg(&mut self) { self.emit_bytes(&[0x48, 0xF7, 0xD8]); } // neg rax

    // ===================================================================
    // Выражения: cmp -> add -> mul -> atom
    // ===================================================================

    fn expr(&mut self) { self.cmp(); }

    fn cmp(&mut self) {
        self.add();
        loop {
            let op = self.tok;
            match op {
                Tk::EqEq | Tk::Ne | Tk::Lt | Tk::Le | Tk::Gt | Tk::Ge => {
                    self.next();
                    self.push_rax();
                    self.add();
                    self.mov_rcx_rax();
                    self.pop_rax();
                    let set2 = match op {
                        Tk::EqEq => 0x94, // sete
                        Tk::Ne => 0x95,   // setne
                        Tk::Lt => 0x92,   // setb (unsigned <)
                        Tk::Le => 0x96,   // setbe (unsigned <=)
                        Tk::Gt => 0x97,   // seta (unsigned >)
                        _ => 0x93,        // setae (unsigned >=)
                    };
                    self.op_cmp_set(set2);
                }
                _ => break,
            }
            if self.err != 0 { break; }
        }
    }

    fn add(&mut self) {
        self.mul();
        loop {
            let op = self.tok;
            match op {
                Tk::Plus | Tk::Minus => {
                    self.next();
                    self.push_rax();
                    self.mul();
                    self.mov_rcx_rax();
                    self.pop_rax();
                    if op == Tk::Plus { self.op_add(); } else { self.op_sub(); }
                }
                _ => break,
            }
            if self.err != 0 { break; }
        }
    }

    fn mul(&mut self) {
        self.atom();
        loop {
            let op = self.tok;
            match op {
                Tk::Star | Tk::Slash => {
                    self.next();
                    self.push_rax();
                    self.atom();
                    self.mov_rcx_rax();
                    self.pop_rax();
                    if op == Tk::Star { self.op_mul(); } else { self.op_div(); }
                }
                _ => break,
            }
            if self.err != 0 { break; }
        }
    }

    fn atom(&mut self) {
        match self.tok {
            Tk::Num => {
                let n = self.num;
                self.next();
                self.mov_rax_imm(n);
            }
            Tk::Ident => {
                let slot = self.var_slot(self.name_off, self.name_len);
                self.next();
                self.load_local(slot);
            }
            Tk::LParen => {
                self.next();
                self.expr();
                self.eat(Tk::RParen);
            }
            Tk::Minus => {
                self.next();
                self.atom();
                self.op_neg();
            }
            _ => { self.fail(E_PARSE); }
        }
    }

    // ===================================================================
    // Операторы
    // ===================================================================

    fn stmt(&mut self) {
        match self.tok {
            Tk::Semi => { self.next(); }
            Tk::Let => {
                self.next();
                if self.tok != Tk::Ident { self.fail(E_PARSE); return; }
                let slot = self.var_slot(self.name_off, self.name_len);
                self.next();
                self.eat(Tk::Eq);
                self.expr();
                self.eat(Tk::Semi);
                self.store_local(slot);
            }
            Tk::Print | Tk::Println => {
                let nl = self.tok == Tk::Println;
                self.next();
                self.expr();
                self.eat(Tk::Semi);
                self.emit_print(nl);
            }
            Tk::If => {
                self.next();
                self.expr();                        // условие -> rax
                self.emit_bytes(&[0x48, 0x85, 0xC0]); // test rax, rax
                self.emit_bytes(&[0x0F, 0x84]);       // je <else/end>
                let site_else = self.clen; self.emit_u32(0);
                self.block();
                if self.tok == Tk::Else {
                    self.next();
                    self.emit_bytes(&[0xE9]);         // jmp <end>
                    let site_end = self.clen; self.emit_u32(0);
                    let else_target = self.clen;
                    self.patch32(site_else, (else_target as i32) - (site_else as i32 + 4));
                    self.block();
                    let end_target = self.clen;
                    self.patch32(site_end, (end_target as i32) - (site_end as i32 + 4));
                } else {
                    let end_target = self.clen;
                    self.patch32(site_else, (end_target as i32) - (site_else as i32 + 4));
                }
            }
            Tk::While => {
                self.next();
                let top = self.clen;
                self.expr();                          // условие -> rax
                self.emit_bytes(&[0x48, 0x85, 0xC0]); // test rax, rax
                self.emit_bytes(&[0x0F, 0x84]);       // je <end>
                let site_end = self.clen; self.emit_u32(0);
                let bbase = self.nbreaks;
                self.block();
                self.emit_bytes(&[0xE9]);             // jmp <top>
                let site_back = self.clen; self.emit_u32(0);
                self.patch32(site_back, (top as i32) - (site_back as i32 + 4));
                let end_target = self.clen;
                self.patch32(site_end, (end_target as i32) - (site_end as i32 + 4));
                self.close_breaks(bbase, end_target);
            }
            Tk::Loop => {
                self.next();
                let top = self.clen;
                let bbase = self.nbreaks;
                self.block();
                self.emit_bytes(&[0xE9]);             // jmp <top>
                let site_back = self.clen; self.emit_u32(0);
                self.patch32(site_back, (top as i32) - (site_back as i32 + 4));
                let end_target = self.clen;
                self.close_breaks(bbase, end_target);
            }
            Tk::Break => {
                self.next();
                self.eat(Tk::Semi);
                self.emit_bytes(&[0xE9]);             // jmp <loop end> (патчится позже)
                let site = self.clen; self.emit_u32(0);
                if self.nbreaks < MAX_BREAKS {
                    self.breaks[self.nbreaks] = site;
                    self.nbreaks += 1;
                } else {
                    self.fail(E_OVERFLOW);
                }
            }
            Tk::Eof | Tk::RBrace => {}
            _ => { self.fail(E_PARSE); }
        }
    }

    /// Запатчить и снять все break-сайты, добавленные с индекса `bbase`.
    fn close_breaks(&mut self, bbase: usize, end_target: usize) {
        let mut i = bbase;
        while i < self.nbreaks {
            let site = self.breaks[i];
            self.patch32(site, (end_target as i32) - (site as i32 + 4));
            i += 1;
        }
        self.nbreaks = bbase;
    }

    fn block(&mut self) {
        self.eat(Tk::LBrace);
        while self.tok != Tk::RBrace && self.tok != Tk::Eof && self.err == 0 {
            self.stmt();
        }
        self.eat(Tk::RBrace);
    }

    // --- пролог/эпилог/print ---

    fn emit_prologue(&mut self) {
        self.emit(0x55);                              // push rbp
        self.emit_bytes(&[0x48, 0x89, 0xE5]);         // mov rbp, rsp
        self.emit_bytes(&[0x48, 0x81, 0xEC]);         // sub rsp, imm32
        self.emit_u32(FRAME_BYTES);
        // Обнулить кадр локалей: xor eax,eax; затем store в каждый слот лениво не
        // делаем — переменные всегда присваиваются перед чтением в корректном
        // коде; неинициализированное чтение вернёт мусор стека (как в C).
    }

    fn emit_print(&mut self, newline: bool) {
        // rax = значение. mov rdi, rax; esi = newline; eax = 32; syscall.
        self.emit_bytes(&[0x48, 0x89, 0xC7]);         // mov rdi, rax
        if newline {
            self.emit_bytes(&[0xBE, 0x01, 0x00, 0x00, 0x00]); // mov esi, 1
        } else {
            self.emit_bytes(&[0x31, 0xF6]);           // xor esi, esi
        }
        self.emit_bytes(&[0xB8, 0x20, 0x00, 0x00, 0x00]); // mov eax, 32
        self.emit_bytes(&[0x0F, 0x05]);               // syscall
    }

    fn emit_exit(&mut self) {
        self.emit_bytes(&[0x31, 0xFF]);               // xor edi, edi  (код выхода 0)
        self.emit_bytes(&[0xB8, 0x06, 0x00, 0x00, 0x00]); // mov eax, 6 (exit)
        self.emit_bytes(&[0x0F, 0x05]);               // syscall
    }

    fn program(&mut self) {
        self.emit_prologue();
        self.next(); // прочитать первый токен
        while self.tok != Tk::Eof && self.err == 0 {
            self.stmt();
        }
        self.emit_exit();
    }
}

// ===================================================================
// Сборка ELF и запуск
// ===================================================================

/// Записать u16/u32/u64 в ELF_BUF по смещению (little-endian).
unsafe fn put16(off: usize, v: u16) { ELF_BUF[off..off + 2].copy_from_slice(&v.to_le_bytes()); }
unsafe fn put32(off: usize, v: u32) { ELF_BUF[off..off + 4].copy_from_slice(&v.to_le_bytes()); }
unsafe fn put64(off: usize, v: u64) { ELF_BUF[off..off + 8].copy_from_slice(&v.to_le_bytes()); }

/// Заполнить минимальный ELF64-заголовок + один PT_LOAD, покрывающий весь файл.
unsafe fn write_elf_headers(total_len: usize) {
    // e_ident
    ELF_BUF[0] = 0x7F; ELF_BUF[1] = b'E'; ELF_BUF[2] = b'L'; ELF_BUF[3] = b'F';
    ELF_BUF[4] = 2; // ELFCLASS64
    ELF_BUF[5] = 1; // ELFDATA2LSB
    ELF_BUF[6] = 1; // EV_CURRENT
    for i in 7..16 { ELF_BUF[i] = 0; }
    put16(16, 2);           // e_type = ET_EXEC
    put16(18, 0x3e);        // e_machine = EM_X86_64
    put32(20, 1);           // e_version
    put64(24, LOAD_VADDR + CODE_OFF as u64); // e_entry -> начало машкода
    put64(32, HDR_SIZE as u64); // e_phoff = 64
    put64(40, 0);           // e_shoff
    put32(48, 0);           // e_flags
    put16(52, HDR_SIZE as u16);  // e_ehsize
    put16(54, PHDR_SIZE as u16); // e_phentsize
    put16(56, 1);           // e_phnum
    put16(58, 0);           // e_shentsize
    put16(60, 0);           // e_shnum
    put16(62, 0);           // e_shstrndx

    // Program header @64: PT_LOAD всего файла в LOAD_VADDR.
    put32(64, 1);           // p_type = PT_LOAD
    put32(68, 7);           // p_flags = R|W|X
    put64(72, 0);           // p_offset = 0 (грузим файл целиком)
    put64(80, LOAD_VADDR);  // p_vaddr
    put64(88, LOAD_VADDR);  // p_paddr
    put64(96, total_len as u64); // p_filesz
    put64(104, total_len as u64); // p_memsz
    put64(112, 0x1000);     // p_align
}

/// Скомпилировать Barrel-исходник и запустить как ring3-процесс.
/// Возвращает PID (>=0) при успехе или отрицательный код ошибки.
pub unsafe fn compile_and_run(src_ptr: *const u8, len: usize) -> i64 {
    let src = core::slice::from_raw_parts(src_ptr, len);

    let mut c = Comp {
        src,
        pos: 0,
        tok: Tk::Eof,
        num: 0,
        name_off: 0,
        name_len: 0,
        clen: 0,
        vars: [(0, 0); MAX_LOCALS],
        nvars: 0,
        breaks: [0; MAX_BREAKS],
        nbreaks: 0,
        err: 0,
    };

    c.program();
    if c.err != 0 {
        return c.err;
    }

    let total = CODE_OFF + c.clen;
    write_elf_headers(total);

    let pid = elf::exec(core::ptr::addr_of!(ELF_BUF) as u64, total as u64);
    if pid < 0 {
        return pid;
    }

    // Запустить процесс синхронно: вернётся, когда тот вызовет exit.
    syscall::run_slot(pid as usize);
    pid
}
