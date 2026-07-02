//! Barrel — встроенный скриптовый язык PureOS.
//!
//! Минималистичный интерпретатор: токенизатор → AST → исполнитель.
//! Zero-Alloc: все буферы статичны. Ограничения:
//!   - макс 64 токена, 32 AST-узла, 16 переменных, 8 уровней вложенности
//!
//! Синтаксис:
//!   print <expr>          — вывод значения
//!   println <expr>        — вывод + новая строка
//!   let <name> = <expr>   — присвоение переменной
//!   input <name>          — чтение строки с клавиатуры
//!   if <cond> { ... } [else { ... }]
//!   loop { ... }
//!   while <cond> { ... }
//!   break
//!   // комментарий
//!
//! Выражения: числа, строки, переменные, сравнения (< > == !=), арифметика (+ -)

use crate::keyboard;
use crate::terminal;

// ===================================================================
// Лексер / Токены
// ===================================================================

#[derive(Copy, Clone, PartialEq)]
enum Token {
    Eof,
    Ident,       // имя переменной / ключевое слово
    Number,      // целое число (u64)
    String,      // строка в кавычках
    Print,
    Println,
    Let,
    Input,
    If,
    Else,
    Loop,
    While,
    Break,
    Eq,          // =
    EqEq,        // ==
    Ne,          // !=
    Lt,          // <
    Gt,          // >
    Plus,        // +
    Minus,       // -
    Star,        // *
    Slash,       // /
    LParen,      // (
    RParen,      // )
    LBrace,      // {
    RBrace,      // }
    Semicolon,   // ;
    Comma,       // ,
}

const MAX_TOKENS: usize = 64;

struct Lexer<'a> {
    src: &'a [u8],
    pos: usize,
}

impl<'a> Lexer<'a> {
    fn new(src: &'a [u8]) -> Self {
        Self { src, pos: 0 }
    }

    fn peek(&self) -> u8 {
        if self.pos >= self.src.len() { 0 } else { self.src[self.pos] }
    }

    fn advance(&mut self) {
        if self.pos < self.src.len() { self.pos += 1; }
    }

    fn skip_whitespace(&mut self) {
        loop {
            let ch = self.peek();
            if ch == b'/' && self.pos + 1 < self.src.len() && self.src[self.pos + 1] == b'/' {
                // Комментарий до конца строки
                while self.peek() != b'\n' && self.peek() != 0 { self.advance(); }
            } else if ch == b' ' || ch == b'\t' || ch == b'\n' || ch == b'\r' {
                self.advance();
            } else {
                break;
            }
        }
    }

    fn tokenize(&mut self, tokens: &mut [Token; MAX_TOKENS], values: &mut [u64; MAX_TOKENS], strbuf: &mut [u8; 256], strpos: &mut usize) -> usize {
        let mut count = 0;
        loop {
            self.skip_whitespace();
            let ch = self.peek();
            if ch == 0 {
                tokens[count] = Token::Eof;
                values[count] = 0;
                count += 1;
                return count;
            }

            let tok = match ch {
                b'"' => {
                    self.advance();
                    let start = *strpos;
                    loop {
                        let c = self.peek();
                        if c == b'"' || c == 0 { break; }
                        if *strpos < 255 { strbuf[*strpos] = c; *strpos += 1; }
                        self.advance();
                    }
                    if self.peek() == b'"' { self.advance(); }
                    strbuf[*strpos] = 0; *strpos += 1;
                    tokens[count] = Token::String;
                    values[count] = start as u64;
                    count += 1;
                    continue;
                }
                b'0'..=b'9' => {
                    let mut n: u64 = 0;
                    while self.peek().is_ascii_digit() {
                        n = n.wrapping_mul(10).wrapping_add((self.peek() - b'0') as u64);
                        self.advance();
                    }
                    tokens[count] = Token::Number;
                    values[count] = n;
                    count += 1;
                    continue;
                }
                b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                    let start = *strpos;
                    while self.peek().is_ascii_alphanumeric() || self.peek() == b'_' {
                        if *strpos < 255 { strbuf[*strpos] = self.peek(); *strpos += 1; }
                        self.advance();
                    }
                    strbuf[*strpos] = 0; *strpos += 1;
                    let name = &strbuf[start..*strpos - 1];
                    let kw = match name {
                        b"print" => Token::Print,
                        b"println" => Token::Println,
                        b"let" => Token::Let,
                        b"input" => Token::Input,
                        b"if" => Token::If,
                        b"else" => Token::Else,
                        b"loop" => Token::Loop,
                        b"while" => Token::While,
                        b"break" => Token::Break,
                        _ => Token::Ident,
                    };
                    tokens[count] = kw;
                    values[count] = start as u64;
                    count += 1;
                    continue;
                }
                b'=' => { self.advance(); if self.peek() == b'=' { self.advance(); Token::EqEq } else { Token::Eq } }
                b'!' => { self.advance(); if self.peek() == b'=' { self.advance(); Token::Ne } else { Token::Eof } }
                b'<' => { self.advance(); Token::Lt }
                b'>' => { self.advance(); Token::Gt }
                b'+' => { self.advance(); Token::Plus }
                b'-' => { self.advance(); Token::Minus }
                b'*' => { self.advance(); Token::Star }
                b'/' => { self.advance(); Token::Slash }
                b'(' => { self.advance(); Token::LParen }
                b')' => { self.advance(); Token::RParen }
                b'{' => { self.advance(); Token::LBrace }
                b'}' => { self.advance(); Token::RBrace }
                b';' => { self.advance(); Token::Semicolon }
                b',' => { self.advance(); Token::Comma }
                _ => { self.advance(); Token::Eof }
            };
            tokens[count] = tok;
            values[count] = 0;
            count += 1;
            if count >= MAX_TOKENS - 1 { break; }
        }
        tokens[count] = Token::Eof;
        values[count] = 0;
        count + 1
    }
}

// ===================================================================
// Переменные
// ===================================================================

const MAX_VARS: usize = 16;

#[derive(Copy, Clone)]
struct Var {
    name_start: usize,
    value: Value,
}

#[derive(Copy, Clone)]
enum Value {
    Num(u64),
    Str(usize),
    Nil,
}

impl Var {
    const fn empty() -> Self {
        Self { name_start: 0, value: Value::Nil }
    }
}

struct Env {
    vars: [Var; MAX_VARS],
    var_count: usize,
    // Указатель на исходный текст скрипта — зарезервировано под строковые
    // переменные (сравнение по содержимому через `str_cmp`).
    #[allow(dead_code)]
    strbuf: *const u8,
    #[allow(dead_code)]
    strbuf_len: usize,
}

impl Env {
    fn new(strbuf: *const u8, strbuf_len: usize) -> Self {
        Self { vars: [Var::empty(); MAX_VARS], var_count: 0, strbuf, strbuf_len }
    }

    fn get(&self, name_start: usize) -> Value {
        for i in 0..self.var_count {
            if self.vars[i].name_start == name_start {
                return self.vars[i].value;
            }
        }
        Value::Nil
    }

    fn set(&mut self, name_start: usize, val: Value) {
        for i in 0..self.var_count {
            if self.vars[i].name_start == name_start {
                self.vars[i].value = val;
                return;
            }
        }
        if self.var_count < MAX_VARS {
            self.vars[self.var_count] = Var { name_start, value: val };
            self.var_count += 1;
        }
    }
}

// ===================================================================
// Интерпретатор
// ===================================================================

/// Главная точка входа: выполнить Barrel-скрипт.
/// Принимает сырой указатель и длину (из статического буфера оболочки).
pub unsafe fn exec(ptr: *const u8, len: usize) {
    let src = core::slice::from_raw_parts(ptr, len);
    let mut strbuf = [0u8; 256];
    let mut strpos: usize = 0;

    let mut tokens = [Token::Eof; MAX_TOKENS];
    let mut values = [0u64; MAX_TOKENS];

    let mut lexer = Lexer::new(src);
    let tcount = lexer.tokenize(&mut tokens, &mut values, &mut strbuf, &mut strpos);

    let mut env = Env::new(strbuf.as_ptr(), strbuf.len());
    let mut ip = 0usize;

    exec_block(&tokens, &values, &mut ip, tcount, &mut env, strbuf.as_ptr(), strbuf.len());
}

unsafe fn exec_block(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    tcount: usize,
    env: &mut Env,
    strbuf: *const u8,
    strbuf_len: usize,
) {
    loop {
        if *ip >= tcount { break; }
        let tok = tokens[*ip];
        match tok {
            Token::Eof | Token::RBrace => { break; }
            Token::Semicolon => { *ip += 1; }
            Token::Print | Token::Println => {
                *ip += 1;
                let val = eval_expr(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                match val {
                    Value::Num(n) => print_num(n),
                    Value::Str(s) => print_str(strbuf, strbuf_len, s),
                    Value::Nil => terminal::write(b"nil"),
                }
                if tok == Token::Println { terminal::putchar(b'\n'); }
                expect_semicolon(tokens, ip, tcount);
            }
            Token::Let => {
                *ip += 1;
                if *ip < tcount && tokens[*ip] == Token::Ident {
                    let name_start = values[*ip] as usize;
                    *ip += 1;
                    if *ip < tcount && tokens[*ip] == Token::Eq {
                        *ip += 1;
                        let val = eval_expr(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                        env.set(name_start, val);
                    }
                }
                expect_semicolon(tokens, ip, tcount);
            }
            Token::Input => {
                *ip += 1;
                if *ip < tcount && tokens[*ip] == Token::Ident {
                    let name_start = values[*ip] as usize;
                    *ip += 1;
                    // Прочитать строку с клавиатуры
                    let mut buf = [0u8; 64];
                    let mut pos = 0;
                    terminal::write(b"> ");
                    loop {
                        if let Some(ch) = keyboard::read_key() {
                            match ch {
                                b'\n' | b'\r' => {
                                    terminal::putchar(b'\n');
                                    break;
                                }
                                0x7F | 0x08 => {
                                    if pos > 0 { pos -= 1; terminal::putchar(0x7F); }
                                }
                                _ if ch >= 0x20 && ch < 0x7F => {
                                    if pos < 63 { buf[pos] = ch; pos += 1; terminal::putchar(ch); }
                                }
                                _ => {}
                            }
                        }
                    }
                    // Поместить строку в strbuf
                    let s_start = strbuf_len.saturating_sub(128);
                    let mut si = s_start;
                    for i in 0..pos {
                        if si < 255 { *(strbuf.add(si) as *mut u8) = buf[i]; si += 1; }
                    }
                    env.set(name_start, Value::Str(s_start));
                }
                expect_semicolon(tokens, ip, tcount);
            }
            Token::If => {
                *ip += 1;
                let cond = eval_expr(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                let truthy = match cond {
                    Value::Num(n) => n != 0,
                    Value::Str(_) => true,
                    Value::Nil => false,
                };
                if *ip < tcount && tokens[*ip] == Token::LBrace {
                    *ip += 1;
                    if truthy {
                        exec_block(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                    } else {
                        skip_block(tokens, ip, tcount);
                    }
                    if *ip < tcount && tokens[*ip] == Token::RBrace { *ip += 1; }
                }
                // else
                if *ip < tcount && tokens[*ip] == Token::Else {
                    *ip += 1;
                    if *ip < tcount && tokens[*ip] == Token::LBrace {
                        *ip += 1;
                        if !truthy {
                            exec_block(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                        } else {
                            skip_block(tokens, ip, tcount);
                        }
                        if *ip < tcount && tokens[*ip] == Token::RBrace { *ip += 1; }
                    }
                }
            }
            Token::Loop => {
                *ip += 1;
                if *ip < tcount && tokens[*ip] == Token::LBrace {
                    *ip += 1;
                    let loop_start = *ip;
                    loop {
                        *ip = loop_start;
                        exec_block(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                        if *ip >= tcount { break; }
                        if tokens[*ip] == Token::RBrace { *ip += 1; break; }
                    }
                }
                if *ip < tcount && tokens[*ip] == Token::RBrace { *ip += 1; }
            }
            Token::While => {
                *ip += 1;
                let cond_start = *ip;
                // Пропустить условие (чтобы найти тело)
                eval_expr(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                let body_start = *ip;
                if *ip < tcount && tokens[*ip] == Token::LBrace {
                    *ip += 1;
                    loop {
                        let mut save_ip = cond_start;
                        let cond = eval_expr(tokens, values, &mut save_ip, tcount, env, strbuf, strbuf_len);
                        let truthy = match cond {
                            Value::Num(n) => n != 0,
                            Value::Str(_) => true,
                            Value::Nil => false,
                        };
                        if !truthy { break; }
                        *ip = body_start + 1;
                        exec_block(tokens, values, ip, tcount, env, strbuf, strbuf_len);
                    }
                    skip_block(tokens, ip, tcount);
                    if *ip < tcount && tokens[*ip] == Token::RBrace { *ip += 1; }
                }
            }
            Token::Break => {
                *ip += 1;
                let mut depth = 1;
                while *ip < tcount && depth > 0 {
                    match tokens[*ip] {
                        Token::LBrace => depth += 1,
                        Token::RBrace => depth -= 1,
                        _ => {}
                    }
                    *ip += 1;
                }
                break;
            }
            _ => { *ip += 1; }
        }
    }
}

unsafe fn expect_semicolon(tokens: &[Token; MAX_TOKENS], ip: &mut usize, _tcount: usize) {
    if *ip < MAX_TOKENS && tokens[*ip] == Token::Semicolon {
        *ip += 1;
    }
}

unsafe fn skip_block(tokens: &[Token; MAX_TOKENS], ip: &mut usize, _tcount: usize) {
    let mut depth = 1;
    while *ip < MAX_TOKENS && depth > 0 {
        match tokens[*ip] {
            Token::LBrace => depth += 1,
            Token::RBrace => depth -= 1,
            _ => {}
        }
        *ip += 1;
    }
}

// Зарезервировано под строковые переменные Barrel (сравнение по содержимому).
#[allow(dead_code)]
fn str_cmp(a: &[u8], b_start: usize, strbuf: *const u8, strbuf_len: usize) -> bool {
    let mut i = 0;
    loop {
        let ca = if i < a.len() { a[i] } else { 0 };
        let cb = if b_start + i < strbuf_len { unsafe { *strbuf.add(b_start + i) } } else { 0 };
        if ca != cb { return false; }
        if ca == 0 { return true; }
        i += 1;
    }
}

// ===================================================================
// Выражения
// ===================================================================

unsafe fn eval_expr(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    _tcount: usize,
    env: &Env,
    strbuf: *const u8,
    strbuf_len: usize,
) -> Value {
    eval_cmp(tokens, values, ip, _tcount, env, strbuf, strbuf_len)
}

unsafe fn eval_cmp(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    _tcount: usize,
    env: &Env,
    strbuf: *const u8,
    strbuf_len: usize,
) -> Value {
    let mut left = eval_add(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
    loop {
        let op = if *ip < MAX_TOKENS { tokens[*ip] } else { Token::Eof };
        match op {
            Token::EqEq | Token::Ne | Token::Lt | Token::Gt => {
                *ip += 1;
                let right = eval_add(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
                let ln = match left { Value::Num(n) => n, _ => 0 };
                let rn = match right { Value::Num(n) => n, _ => 0 };
                left = Value::Num(match op {
                    Token::EqEq => if ln == rn { 1 } else { 0 },
                    Token::Ne => if ln != rn { 1 } else { 0 },
                    Token::Lt => if ln < rn { 1 } else { 0 },
                    Token::Gt => if ln > rn { 1 } else { 0 },
                    _ => 0,
                });
            }
            _ => break,
        }
    }
    left
}

unsafe fn eval_add(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    _tcount: usize,
    env: &Env,
    strbuf: *const u8,
    strbuf_len: usize,
) -> Value {
    let mut left = eval_mul(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
    loop {
        let op = if *ip < MAX_TOKENS { tokens[*ip] } else { Token::Eof };
        match op {
            Token::Plus | Token::Minus => {
                *ip += 1;
                let right = eval_mul(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
                let ln = match left { Value::Num(n) => n, _ => 0 };
                let rn = match right { Value::Num(n) => n, _ => 0 };
                left = Value::Num(if op == Token::Plus { ln.wrapping_add(rn) } else { ln.wrapping_sub(rn) });
            }
            _ => break,
        }
    }
    left
}

unsafe fn eval_mul(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    _tcount: usize,
    env: &Env,
    strbuf: *const u8,
    strbuf_len: usize,
) -> Value {
    let mut left = eval_atom(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
    loop {
        let op = if *ip < MAX_TOKENS { tokens[*ip] } else { Token::Eof };
        match op {
            Token::Star | Token::Slash => {
                *ip += 1;
                let right = eval_atom(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
                let ln = match left { Value::Num(n) => n, _ => 0 };
                let rn = match right { Value::Num(n) => n, _ => 0 };
                left = Value::Num(if op == Token::Star { ln.wrapping_mul(rn) } else { if rn != 0 { ln / rn } else { 0 } });
            }
            _ => break,
        }
    }
    left
}

unsafe fn eval_atom(
    tokens: &[Token; MAX_TOKENS],
    values: &[u64; MAX_TOKENS],
    ip: &mut usize,
    _tcount: usize,
    env: &Env,
    strbuf: *const u8,
    strbuf_len: usize,
) -> Value {
    if *ip >= MAX_TOKENS { return Value::Nil; }
    let tok = tokens[*ip];
    *ip += 1;
    match tok {
        Token::Number => Value::Num(values[*ip - 1]),
        Token::String => {
            let s = values[*ip - 1] as usize;
            Value::Str(s)
        }
        Token::Ident => {
            let name_start = values[*ip - 1] as usize;
            env.get(name_start)
        }
        Token::LParen => {
            let val = eval_expr(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
            if *ip < MAX_TOKENS && tokens[*ip] == Token::RParen { *ip += 1; }
            val
        }
        Token::Minus => {
            let val = eval_atom(tokens, values, ip, _tcount, env, strbuf, strbuf_len);
            match val { Value::Num(n) => Value::Num(n.wrapping_neg()), _ => Value::Nil }
        }
        _ => Value::Nil,
    }
}

// ===================================================================
// Вывод значений
// ===================================================================

unsafe fn print_str(strbuf: *const u8, strbuf_len: usize, start: usize) {
    let mut p = start;
    loop {
        if p >= strbuf_len { break; }
        let ch = *strbuf.add(p);
        if ch == 0 { break; }
        terminal::putchar(ch);
        p += 1;
    }
}

unsafe fn print_num(n: u64) {
    terminal::write_num(n);
}
