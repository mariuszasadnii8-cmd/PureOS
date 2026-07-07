//! Звуковая подсистема PureOS.
//!
//! High-level API поверх PC Speaker.
//! Поддерживает: ноты по имени, мелодии, событийные звуки,
//! глобальное вкл/выкл, громкость.

use crate::pcspeaker;

/// Состояние звуковой подсистемы.
static mut SOUND_ENABLED: bool = true;
static mut SOUND_VOLUME: u8 = 100; // 0..100

/// Частоты нот (C4..B4) в Гц. Индекс = полутон от C4 (0=C, 1=C#, ..., 11=B).
const NOTE_FREQS_4: [u32; 12] = [
    262, 277, 294, 311, 330, 349, 370, 392, 415, 440, 466, 494,
];

/// Имена нот для парсинга.
const NOTE_NAMES: &[u8] = b"CCDDEFFGGAAB";
const NOTE_ACCIDENTALS: &[u8] = b"# # #  # # #  ";

/// Включить/выключить звук.
pub unsafe fn set_enabled(on: bool) {
    SOUND_ENABLED = on;
    if !on { pcspeaker::off(); }
}

/// Проверить, включен ли звук.
pub fn is_enabled() -> bool { unsafe { SOUND_ENABLED } }

/// Установить громкость (0..100).
pub unsafe fn set_volume(v: u8) {
    SOUND_VOLUME = v.min(100);
}

/// Получить громкость.
pub fn get_volume() -> u8 { unsafe { SOUND_VOLUME } }

/// Преобразовать имя ноты (например "C4") в частоту.
/// Формат: буква (C,D,E,F,G,A,B) + опционально # или b + октава (0..8).
/// Если октава не указана — 4.
/// Возвращает частоту в Гц или 0 при ошибке.
pub fn note_freq(name: &[u8]) -> u32 {
    if name.is_empty() { return 0; }
    let ch = name[0].to_ascii_uppercase();
    let mut semitone: Option<usize> = None;
    for i in 0..12 {
        if NOTE_NAMES[i] == ch {
            semitone = Some(i);
            break;
        }
    }
    let base = match semitone {
        Some(s) => s,
        None => return 0,
    };

    let mut offset = 1;
    // Check for accidental
    if name.len() > 1 && (name[1] == b'#' || name[1] == b'b') {
        let acc = name[1];
        if offset + 1 >= name.len() { return 0; }
        if acc == b'#' && base + 1 < 12 {
            // next semitone
        } else if acc == b'b' && base > 0 {
            // prev semitone
        } else {
            return 0;
        }
        offset = 2;
    }

    // Parse octave
    let octave = if offset < name.len() {
        let d = name[offset];
        if d >= b'0' && d <= b'8' { (d - b'0') as i32 } else { 4 }
    } else {
        4
    };

    // Calculate: freq = 440 * 2^((semitone - 57) / 12)
    // semitone 0 = C4 = 262 Hz, semitone 57 = A4 = 440 Hz
    let semi_idx = base as i32 + (octave - 4) * 12;
    if semi_idx < 0 || semi_idx > 96 { return 0; }

    let freq = NOTE_FREQS_4[base];
    if octave >= 4 {
        freq << (octave - 4)
    } else {
        freq >> (4 - octave)
    }
}

/// Сыграть ноту по частоте (Гц) и длительности (мс).
pub unsafe fn play_freq(freq_hz: u32, ms: u32) {
    if !SOUND_ENABLED || freq_hz == 0 || ms == 0 { return; }
    let vol = SOUND_VOLUME as u32;
    // Volume control: simulate by reducing duration
    let effective_ms = (ms * vol) / 100;
    if effective_ms == 0 { return; }
    pcspeaker::beep(freq_hz, effective_ms);
}

/// Сыграть ноту по имени (например "C4", "F#3", "Bb5").
pub unsafe fn play_note(name: &[u8], ms: u32) {
    let freq = note_freq(name);
    if freq == 0 { return; }
    play_freq(freq, ms);
}

/// Сыграть мелодию из пар (нота, длительность_мс).
/// Нота может быть именем ("C4") или числом (частота Гц).
pub unsafe fn play_melody(melody: &[&[u8]]) {
    let mut i = 0;
    while i + 1 < melody.len() {
        let note = melody[i];
        let dur_str = melody[i + 1];
        let mut ms = 200u32;
        // parse duration
        if !dur_str.is_empty() {
            let mut n = 0u32;
            for &ch in dur_str {
                if ch >= b'0' && ch <= b'9' {
                    n = n * 10 + (ch - b'0') as u32;
                }
            }
            if n > 0 { ms = n.min(10000); }
        }
        // Try as note name first, then as raw frequency
        match note_freq(note) {
            0 => {
                // Try as raw number
                let mut freq = 0u32;
                for &ch in note {
                    if ch >= b'0' && ch <= b'9' {
                        freq = freq * 10 + (ch - b'0') as u32;
                    }
                }
                if freq > 0 { play_freq(freq.min(20000), ms); }
            }
            f => play_freq(f, ms),
        }
        i += 2;
    }
}

// ═══════════════════════════════════════════════════════════════════
// System Event Sounds
// ═══════════════════════════════════════════════════════════════════

/// Звук загрузки — короткое приветствие.
pub unsafe fn boot() {
    if !SOUND_ENABLED { return; }
    play_freq(523, 80);  // C5
    play_freq(659, 80);  // E5
    play_freq(784, 120); // G5
}

/// Звук ошибки — два коротких низких.
pub unsafe fn error() {
    if !SOUND_ENABLED { return; }
    play_freq(330, 100); // E4
    play_freq(262, 150); // C4
}

/// Звук нажатия клавиши — короткий тихий щелчок.
pub unsafe fn click() {
    if !SOUND_ENABLED { return; }
    play_freq(2000, 5);
}

/// Уведомление — приятный двухтоновый сигнал.
pub unsafe fn notification() {
    if !SOUND_ENABLED { return; }
    play_freq(880, 60);  // A5
    play_freq(1108, 80); // C#6
}

/// Звук выключения — нисходящая гамма.
pub unsafe fn shutdown() {
    if !SOUND_ENABLED { return; }
    play_freq(784, 80);  // G5
    play_freq(659, 80);  // E5
    play_freq(523, 80);  // C5
    play_freq(392, 150); // G4
}

/// Короткий сигнал подтверждения (например после успешной команды).
pub unsafe fn confirm() {
    if !SOUND_ENABLED { return; }
    play_freq(1047, 40);  // C6
    play_freq(1319, 60);  // E6
}

/// Звук запуска приложения.
pub unsafe fn app_open() {
    if !SOUND_ENABLED { return; }
    play_freq(660, 50);
    play_freq(880, 70);
}

/// Звук закрытия окна.
pub unsafe fn app_close() {
    if !SOUND_ENABLED { return; }
    play_freq(440, 40);
    play_freq(330, 60);
}

/// Воспроизвести мелодию из строки формата:
/// "NOTE1 DUR1 NOTE2 DUR2 ..."
/// Где NOTE = нота+октава (C4, F#5, Bb3) или частота (440)
/// DUR = длительность в мс (200)
pub unsafe fn play_string(s: &[u8]) {
    if !SOUND_ENABLED { return; }
    // Разбиваем на токены
    let mut tokens: [&[u8]; 32] = [b""; 32];
    let mut count = 0;
    let mut start = 0;
    for i in 0..=s.len() {
        if i == s.len() || s[i] == b' ' {
            if i > start {
                if count < 32 {
                    tokens[count] = &s[start..i];
                    count += 1;
                }
            }
            start = i + 1;
        }
    }
    play_melody(&tokens[..count]);
}
