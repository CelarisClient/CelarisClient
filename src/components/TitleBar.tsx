import { getCurrentWindow } from "@tauri-apps/api/window";

const appWindow = getCurrentWindow();

/** Frameless custom title bar: draggable surface + window controls. */
export function TitleBar() {
  return (
    <div className="titlebar" data-tauri-drag-region>
      <div className="titlebar-brand" data-tauri-drag-region>
        <span className="titlebar-dot" />
        CELARIS&nbsp;LAUNCHER
      </div>

      <div className="titlebar-controls">
        <button className="winbtn" aria-label="Minimieren" onClick={() => appWindow.minimize()}>
          <svg width="11" height="11" viewBox="0 0 11 11"><rect x="1" y="5" width="9" height="1" fill="currentColor" /></svg>
        </button>
        <button className="winbtn" aria-label="Maximieren" onClick={() => appWindow.toggleMaximize()}>
          <svg width="11" height="11" viewBox="0 0 11 11" fill="none"><rect x="1.5" y="1.5" width="8" height="8" stroke="currentColor" /></svg>
        </button>
        <button className="winbtn winbtn--close" aria-label="Schließen" onClick={() => appWindow.close()}>
          <svg width="11" height="11" viewBox="0 0 11 11"><path d="M1 1l9 9M10 1l-9 9" stroke="currentColor" strokeWidth="1.1" /></svg>
        </button>
      </div>
    </div>
  );
}
