//! Test Runner — встроенный набор тестов для ядра PureOS.
//!
//! Запуск: команда `test` в оболочке.
//! Тестирует: frame alloc, FS, Barrel, syscall.
//! Zero-Alloc: все буферы статические.

use crate::frame;
use crate::fs;
use crate::keyboard;
use crate::syscall;
use crate::terminal;

/// Запустить все тесты. Возвращается после завершения.
pub unsafe fn run() {
    terminal::clear();
    terminal::write(b"=== PureOS Test Suite ===\n\n");
    let mut failed = 0u32;

    // ── 1. Frame allocator ──
    terminal::write(b"[frame]\n");
    check(b"alloc_frame", &mut failed, || frame::alloc_frame().is_some());
    check(b"stats", &mut failed, || {
        let s = frame::stats();
        s.total_frames > 0
    });

    // ── 2. Filesystem ──
    terminal::write(b"\n[fs]\n");
    check(b"root is Dir", &mut failed, || fs::kind(fs::ROOT) == fs::Kind::Dir);
    let _ = fs::mkdir(fs::ROOT, b"td");
    let td = fs::mkdir(fs::ROOT, b"td2").unwrap();
    let f = fs::create_file(td, b"a.txt").unwrap();
    check(b"create file", &mut failed, || f != fs::ROOT);
    check(b"write file", &mut failed, || fs::write(f, b"hello world"));
    check(b"read file", &mut failed, || {
        let data = fs::read(f);
        data == b"hello world"
    });
    check(b"resolve /td2", &mut failed, || fs::resolve(b"/td2") == Some(td));
    check(b"resolve /td2/a.txt", &mut failed, || fs::resolve(b"/td2/a.txt") == Some(f));
    check(b"resolve_parent", &mut failed, || {
        let (parent, leaf) = fs::resolve_parent(b"/td2/a.txt").unwrap();
        leaf == b"a.txt" && parent == td
    });
    check(b"path_of /", &mut failed, || {
        let mut buf = [0u8; 256];
        let n = fs::path_of(fs::ROOT, &mut buf);
        n == 1 && buf[0] == b'/'
    });
    check(b"unlink file", &mut failed, || fs::unlink(f));

    // ── 3. Barrel interpreter ──
    terminal::write(b"\n[barrel]\n");
    check(b"barrel exec", &mut failed, || {
        crate::barrel::exec(b"print 42 ;\n" as *const u8, 11);
        true
    });

    // ── 4. Process table ──
    terminal::write(b"\n[syscall]\n");
    check(b"process 0 exists", &mut failed, || syscall::PROCESS_TABLE[0].id == 0);
    check(b"find_free_slot", &mut failed, || syscall::find_free_slot().is_some());

    terminal::write(b"\n=== Results ===\n");
    if failed == 0 {
        terminal::write(b"All tests PASSED\n");
    } else {
        terminal::write(b"FAILED: ");
        terminal::write_num(failed as u64);
        terminal::write(b"\n");
    }
    terminal::write(b"Press any key to continue...\n");

    loop {
        keyboard::poll();
        if keyboard::read_key().is_some() { break; }
        core::arch::asm!("pause", options(nomem, nostack, preserves_flags));
    }
    terminal::clear();
}

unsafe fn check(name: &[u8], failed: &mut u32, f: impl FnOnce() -> bool) {
    terminal::write(b"  ");
    terminal::write(name);
    terminal::write(b"... ");
    if f() {
        terminal::write(b"PASS\n");
    } else {
        terminal::write(b"FAIL\n");
        *failed += 1;
    }
}
