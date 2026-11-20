import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { getVersion } from '@tauri-apps/api/app';

export const isDesktopApp = (): boolean =>
  typeof window !== 'undefined' && '__TAURI_INTERNALS__' in window;

export const desktopApi = {
  async getAppVersion(): Promise<string> {
    if (!isDesktopApp()) return '1.1.2';
    try {
      return await getVersion();
    } catch {
      return '1.1.2';
    }
  },

  getDefaultSaveInfo(fallbackPath?: string, prefix?: string) {
    return invoke<{ savePath: string; currentIndex: number; preferences?: any }>(
      'get_default_save_info',
      { fallbackPath: fallbackPath || null, prefix: prefix || '' }
    );
  },

  savePreferences(patch: Record<string, any>) {
    return invoke<any>('save_preferences', { patch });
  },

  getOpenWindows() {
    return invoke<Array<{ id: string; title: string; appName: string; icon: string; width: number; height: number }>>(
      'get_open_windows'
    );
  },

  chooseFolder(prefix?: string) {
    return invoke<{ savePath: string; currentIndex: number } | null>(
      'choose_folder',
      { prefix: prefix || '' }
    );
  },

  selectRegion(lang?: string) {
    return invoke<{ x?: number; y?: number; width?: number; height?: number; cancelled?: boolean }>(
      'select_region',
      { lang: lang || 'en' }
    );
  },

  startRecord(options: any) {
    return invoke<any>('start_record', { options });
  },

  pauseRecord() {
    return invoke<void>('pause_record');
  },

  resumeRecord() {
    return invoke<void>('resume_record');
  },

  stopSave() {
    return invoke<{ path: string; fileName: string; durationSeconds: number; currentIndex: number } | null>(
      'stop_save'
    );
  },

  cancelRecord() {
    return invoke<{ cancelled: boolean }>('cancel_record');
  },

  onRecordingStatusChange(callback: (event: { status: string }) => void) {
    let unlisten: (() => void) | null = null;
    listen<{ status: string }>('recording-status-change', (e) => callback(e.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => { if (unlisten) unlisten(); };
  },

  onRecordingSaved(callback: (result: any) => void) {
    let unlisten: (() => void) | null = null;
    listen<any>('recording-saved', (e) => callback(e.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => { if (unlisten) unlisten(); };
  },

  onRecordingCancelled(callback: () => void) {
    let unlisten: (() => void) | null = null;
    listen('recording-cancelled', () => callback()).then((fn) => {
      unlisten = fn;
    });
    return () => { if (unlisten) unlisten(); };
  },

  onHotkeyStart(callback: () => void) {
    let unlisten: (() => void) | null = null;
    listen('hotkey-start', () => callback()).then((fn) => {
      unlisten = fn;
    });
    return () => { if (unlisten) unlisten(); };
  },

  onRecordingError(callback: (message: string) => void) {
    let unlisten: (() => void) | null = null;
    listen<string>('recording-error', (e) => callback(e.payload)).then((fn) => {
      unlisten = fn;
    });
    return () => { if (unlisten) unlisten(); };
  },
};
