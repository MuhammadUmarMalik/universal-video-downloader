const { app, BrowserWindow, ipcMain, session } = require("electron");
const { spawn } = require("node:child_process");
const path = require("node:path");
const readline = require("node:readline");

const COMMANDS = new Set([
  "get_foundation_status",
  "analyze_url",
  "create_download",
  "get_bandwidth_status",
  "set_bandwidth_limit",
  "cancel_download",
  "get_download_jobs",
  "subscribe_download_progress",
  "get_history",
  "delete_history_entry",
  "clear_history",
  "get_schedules",
  "create_schedule",
  "update_schedule",
  "delete_schedule",
  "get_scheduler_enabled",
  "set_scheduler_enabled",
  "run_scheduler_now",
]);

class RustRpcClient {
  constructor() {
    this.nextId = 1;
    this.pending = new Map();
    this.window = null;
    this.child = null;
  }

  start() {
    const packagedBinary = process.resourcesPath
      ? path.join(process.resourcesPath, "universal-media-downloader")
      : null;
    const developmentBinary = path.join(
      __dirname,
      "..",
      "src-rust",
      "target",
      "release",
      process.platform === "win32" ? "universal-media-downloader.exe" : "universal-media-downloader",
    );
    const binary = process.env.UMD_RUST_BINARY || (packagedBinary && require("node:fs").existsSync(packagedBinary) ? packagedBinary : developmentBinary);
    this.child = spawn(binary, ["--headless"], {
      cwd: app.getPath("userData"),
      env: {
        ...process.env,
        UMD_APP_DATA_DIR: app.getPath("userData"),
      },
      stdio: ["pipe", "pipe", "pipe"],
      windowsHide: true,
    });

    const lines = readline.createInterface({ input: this.child.stdout });
    lines.on("line", (line) => this.handleLine(line));
    this.child.stderr.on("data", (chunk) => {
      process.stderr.write(`[umd-rust] ${chunk}`);
    });
    this.child.on("error", (error) => this.failPending(`Rust core could not be started: ${error.message}`));
    this.child.on("exit", (code, signal) => {
      this.failPending(`Rust core exited unexpectedly (${code ?? "unknown"}${signal ? `, ${signal}` : ""}).`);
      if (this.window && !this.window.isDestroyed()) {
        this.window.webContents.send("desktop-bridge-error", { message: "The Rust desktop core stopped." });
      }
    });
  }

  attachWindow(window) {
    this.window = window;
  }

  handleLine(line) {
    let message;
    try {
      message = JSON.parse(line);
    } catch {
      return;
    }
    if (message.event === "download-progress") {
      if (this.window && !this.window.isDestroyed()) {
        this.window.webContents.send("download-progress", message.payload);
      }
      return;
    }
    const pending = this.pending.get(String(message.id));
    if (!pending) return;
    this.pending.delete(String(message.id));
    if (message.ok) pending.resolve(message.result);
    else pending.reject(message.error || { message: "The local desktop core returned an unknown error." });
  }

  request(command, args = {}) {
    if (!COMMANDS.has(command)) return Promise.reject(new Error("Unsupported desktop command."));
    if (!this.child || !this.child.stdin.writable) return Promise.reject(new Error("The Rust desktop core is unavailable."));
    const id = String(this.nextId++);
    return new Promise((resolve, reject) => {
      this.pending.set(id, { resolve, reject });
      this.child.stdin.write(`${JSON.stringify({ id, command, args })}\n`, (error) => {
        if (error) {
          this.pending.delete(id);
          reject(error);
        }
      });
    });
  }

  failPending(message) {
    for (const { reject } of this.pending.values()) reject(new Error(message));
    this.pending.clear();
  }

  stop() {
    if (this.child && !this.child.killed) this.child.kill();
    this.failPending("The Rust desktop core stopped.");
  }
}

let mainWindow;
let rpc;

function createWindow() {
  mainWindow = new BrowserWindow({
    width: 1280,
    height: 820,
    minWidth: 1024,
    minHeight: 680,
    backgroundColor: "#f8fafc",
    webPreferences: {
      preload: path.join(__dirname, "preload.cjs"),
      contextIsolation: true,
      nodeIntegration: false,
      sandbox: true,
    },
  });
  rpc.attachWindow(mainWindow);
  if (process.env.ELECTRON_START_URL) {
    void mainWindow.loadURL(process.env.ELECTRON_START_URL);
  } else {
    void mainWindow.loadFile(path.join(__dirname, "..", "dist", "index.html"));
  }
  mainWindow.on("closed", () => {
    mainWindow = null;
    rpc.window = null;
  });
}

app.whenReady().then(() => {
  session.defaultSession.webRequest.onHeadersReceived((details, callback) => {
    callback({
      responseHeaders: {
        ...details.responseHeaders,
        "Content-Security-Policy": ["default-src 'self'; style-src 'self' 'unsafe-inline'; script-src 'self'; img-src 'self' data:; connect-src 'self'; font-src 'self' data:"],
      },
    });
  });
  rpc = new RustRpcClient();
  rpc.start();
  ipcMain.handle("desktop-rpc", (_event, command, args) => rpc.request(command, args));
  createWindow();
});

app.on("window-all-closed", () => {
  rpc?.stop();
  if (process.platform !== "darwin") app.quit();
});

app.on("before-quit", () => rpc?.stop());
