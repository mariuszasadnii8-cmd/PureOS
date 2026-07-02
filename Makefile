PROJECT      := PureOS
BUILD_DIR    := build
ESP_DIR      := $(BUILD_DIR)/esp

KERNEL_TARGET := x86_64-unknown-none
UEFI_TARGET   := x86_64-unknown-uefi

KERNEL_ELF    := kernel/target/$(KERNEL_TARGET)/release/pureos_kernel
UEFI_APP      := uefi_boot/target/$(UEFI_TARGET)/release/pureos_uefi_boot.efi

# Прямые явные пути без ломающегося автопоиска через скрипты
QEMU         := "C:\Program Files\qemu\qemu-system-x86_64.exe"
QEMU_MEMORY  := 512M
QEMU_DISPLAY := sdl

SHELL := powershell.exe
.SHELLFLAGS := -NoProfile -ExecutionPolicy Bypass -Command

.PHONY: all kernel uefi user-programs esp run test dev clean distclean help

all: run

help:
	Write-Host "PureOS Build Targets:"
	Write-Host "  make kernel       — сборка ядра (x86_64-unknown-none)"
	Write-Host "  make uefi         — сборка UEFI-загрузчика"
	Write-Host "  make esp          — сборка ядра+загрузчика + ESP-директория"
	Write-Host "  make run          — запуск в QEMU напрямую из папки"
	Write-Host "  make clean        — cargo clean + удалить build/"
	Write-Host "  make distclean    — clean + Cargo.lock"

kernel:
	Set-Location kernel; cargo build --release --target $(KERNEL_TARGET)

uefi:
	Set-Location uefi_boot; cargo build --release --target $(UEFI_TARGET)

user-programs:
	Write-Host "  [SKIP] userspace not linked into kernel yet"

esp: kernel uefi
	New-Item -ItemType Directory -Force -Path '$(ESP_DIR)/EFI/BOOT' | Out-Null
	New-Item -ItemType Directory -Force -Path '$(ESP_DIR)/EFI/PUREOS' | Out-Null
	Copy-Item -Force '$(UEFI_APP)' '$(ESP_DIR)/EFI/BOOT/BOOTX64.EFI'
	Copy-Item -Force '$(KERNEL_ELF)' '$(ESP_DIR)/EFI/PUREOS/KERNEL.ELF'
	# startup.nsh — автостарт bootloader через UEFI Shell
	Set-Content -Path '$(ESP_DIR)/startup.nsh' -Value 'fs0:\EFI\BOOT\BOOTX64.EFI' -Encoding Ascii

run: esp
	& $(QEMU) -machine q35 -cpu qemu64,+syscall -m $(QEMU_MEMORY) -pflash "C:\Program Files\qemu\share\edk2-x86_64-code.fd" -drive format=raw,file=fat:rw:$(ESP_DIR) -serial stdio -display sdl -vga std -no-reboot

dev: run

test: esp
	& $(QEMU) -machine q35 -cpu qemu64,+syscall -m $(QEMU_MEMORY) -pflash "C:\Program Files\qemu\share\edk2-x86_64-code.fd" -drive format=raw,file=fat:rw:$(ESP_DIR) -serial stdio -display sdl -vga std -no-reboot -d cpu_reset,int 2>&1

clean:
	Set-Location kernel; cargo clean
	Set-Location uefi_boot; cargo clean
	Remove-Item -Recurse -Force '$(BUILD_DIR)' -ErrorAction SilentlyContinue

distclean: clean
	Remove-Item -Force 'uefi_boot/Cargo.lock','kernel/Cargo.lock' -ErrorAction SilentlyContinue
flash: esp
	Write-Host "-> Copying PureOS files to USB drive..."
	Copy-Item -Force -Recurse '$(ESP_DIR)/*' 'E:/'
	Write-Host "-> Done! You can now unplug your USB drive."