# LX350 Panel

A desktop-style printer control panel app for the Epson LX-350 dot matrix printer. Built with **Tauri**, React, TypeScript, Tailwind CSS v4, and Radix UI-inspired components for a dark industrial aesthetic.

## Features
- **Port Selection**: Automatically lists available COM & USB ports and allows connection.
- **Printer Commands Panel**: Send common ESC/P commands (Line Feed, Form Feed, Tear Off, Initialize).
- **Font Selector**: Quickly switch between Draft, Roman, and Sans Serif printer fonts.
- **Micro Adjust**: Fine-tune paper positioning using ESC + and ESC - commands.
- **Command Log Terminal**: A green-on-dark CRT-style terminal logging hex bytes and status.
- **Clean Architecture**: Encapsulates all `serialport` logic in a robust Rust backend to ensure performance and reliability.

## Prerequisites
- Node.js (v18+ recommended)
- Rust (`rustc` and `cargo` via rustup.rs)
- A connected Epson LX-350 (or compatible ESC/P dot matrix printer) via Serial/USB.

## Setup & Build Instructions

1. **Install Dependencies**
   ```bash
   pnpm install
   ```

2. **Run Development Mode (Local Testing)**
   Starts the Vite dev server and opens the Tauri window. Hot-reloading is supported.
   ```bash
   pnpm dev
   ```

3. **Build the Windows Executable (.msi/.exe)**
   This command will compile the React app and build the Rust backend via Tauri, packaging everything into an installer.
   ```bash
   pnpm build
   ```
   The final executable will be located in the `src-tauri/target/release/bundle/` folder.

## Architecture Notes
- **`src-tauri/src/lib.rs`**: Contains the Rust backend handling the `serialport` crate logic. This ensures highly efficient hardware communication.
- **`src/App.tsx`**: Uses a custom dark industrial theme heavily inspired by Shadcn UI and Radix UI primitives, ensuring high-quality modular design. It communicates with the Rust backend via `@tauri-apps/api/core/invoke`.
