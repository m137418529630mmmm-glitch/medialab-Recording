import React, { useState } from 'react';
import {
  Pause,
  Play,
  Square,
  Trash2,
  Camera,
  EyeOff,
  Move,
  Eye,
  X,
} from 'lucide-react';
import {
  StorageSettings,
  ToggleSettings,
  RegionCoordinates,
  CaptureMode,
} from '../types/recorder';
import { Language, translations } from '../utils/translations';

interface LiveRecordingOverlayProps {
  lang: Language;
  isActive: boolean;
  status: 'idle' | 'countdown' | 'recording' | 'paused';
  timerFormatted: string;
  secondsElapsed: number;
  countdownValue: number;
  storage: StorageSettings;
  toggles: ToggleSettings;
  region: RegionCoordinates;
  captureMode: CaptureMode;
  onPauseResume: () => void;
  onSave: () => void;
  onCancel: () => void;
  onTakeSnapshot: () => void;
  onShowToast: (msg: string) => void;
}

export const LiveRecordingOverlay: React.FC<LiveRecordingOverlayProps> = ({
  lang,
  isActive,
  status,
  timerFormatted,
  secondsElapsed,
  countdownValue,
  toggles,
  region,
  captureMode,
  onPauseResume,
  onSave,
  onCancel,
  onTakeSnapshot,
  onShowToast,
}) => {
  const t = translations[lang];
  const [hudOffset, setHudOffset] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isDraggingHud, setIsDraggingHud] = useState(false);
  const [dragStart, setDragStart] = useState<{ x: number; y: number }>({ x: 0, y: 0 });
  const [isBarVisible, setIsBarVisible] = useState<boolean>(true);

  const targetSecs = toggles.autoStopEnabled
    ? (toggles.autoStopHours || 0) * 3600 +
      (toggles.autoStopMinutes || 0) * 60 +
      (toggles.autoStopSeconds || 0)
    : 0;

  const formatTargetTime = (secs: number) => {
    const hrs = Math.floor(secs / 3600);
    const mins = Math.floor((secs % 3600) / 60);
    const s = secs % 60;
    if (hrs > 0) {
      return `${String(hrs).padStart(2, '0')}:${String(mins).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
    }
    return `${String(mins).padStart(2, '0')}:${String(s).padStart(2, '0')}`;
  };

  const progressPercent = targetSecs > 0 ? Math.min(100, (secondsElapsed / targetSecs) * 100) : 0;

  if (!isActive) return null;

  const handleMouseDown = (e: React.MouseEvent) => {
    setIsDraggingHud(true);
    setDragStart({ x: e.clientX - hudOffset.x, y: e.clientY - hudOffset.y });
  };

  const handleMouseMove = (e: React.MouseEvent) => {
    if (!isDraggingHud) return;
    setHudOffset({
      x: e.clientX - dragStart.x,
      y: e.clientY - dragStart.y,
    });
  };

  const handleMouseUp = () => {
    setIsDraggingHud(false);
  };

  const handleHideBar = () => {
    setIsBarVisible(false);
    onShowToast(t.barHiddenToast);
  };

  return (
    <div
      onMouseMove={handleMouseMove}
      onMouseUp={handleMouseUp}
      className="fixed inset-0 z-50 pointer-events-auto flex flex-col justify-between p-6 select-none"
    >
      <div className="absolute inset-0 bg-black/40 backdrop-blur-[1px] -z-10" />

      {status === 'countdown' && (
        <div className="absolute inset-0 flex flex-col items-center justify-center bg-black/60 backdrop-blur-md z-30">
          <div className="text-center space-y-4 animate-in zoom-in-75 duration-200">
            <div className="font-mono text-8xl font-black text-[#D85631] animate-pulse">
              {countdownValue}
            </div>
            <div className="text-sm font-bold text-white">
              {lang === 'en' ? 'Preparing to record...' : 'الاستعداد لبدء التسجيل...'}
            </div>
            <button
              type="button"
              onClick={onCancel}
              className="px-4 py-1.5 rounded-lg bg-white/15 hover:bg-white/25 text-white text-xs font-bold cursor-pointer"
            >
              {t.cancelRecording}
            </button>
          </div>
        </div>
      )}

      {captureMode === 'region' ? (
        <div
          className="absolute rounded-xl border-[3px] border-solid border-[#000000] shadow-none pointer-events-none transition-all duration-200"
          style={{
            left: `${Math.max(20, Math.min(window.innerWidth - 300, region.x / 2))}px`,
            top: `${Math.max(20, Math.min(window.innerHeight - 200, region.y / 2))}px`,
            width: `${Math.min(window.innerWidth - 60, region.width / 2)}px`,
            height: `${Math.min(window.innerHeight - 100, region.height / 2)}px`,
          }}
        >
          <div className="absolute -top-6 right-0 bg-black text-white font-mono text-[10px] font-bold px-2 py-0.5 rounded shadow-none" dir="ltr">
            REC {region.width} × {region.height} px
          </div>
        </div>
      ) : (
        <div className="absolute inset-3 rounded-2xl border-2 border-[#D85631]/70 pointer-events-none" />
      )}

      {!isBarVisible && (
        <div className="self-center mb-4 z-40 animate-in fade-in zoom-in-95 duration-150">
          <button
            type="button"
            onClick={() => setIsBarVisible(true)}
            className="flex items-center gap-2.5 bg-[#FFFFFF] hover:bg-[#F9F8F6] border border-[#E5E2DC] hover:border-[#D85631] px-4 py-2 rounded-full shadow-2xl text-xs font-bold text-[#1E1D1B] cursor-pointer transition-all"
            title={t.showBar}
          >
            <span
              className={`w-2.5 h-2.5 rounded-full ${
                status === 'recording' ? 'bg-[#D85631] animate-ping' : 'bg-amber-500'
              }`}
            />
            <span className="font-mono tabular-nums" dir="ltr">{timerFormatted}</span>
            {toggles.autoStopEnabled && targetSecs > 0 && (
              <span className="text-[10px] font-mono text-[#7E7B75]" dir="ltr">
                / {formatTargetTime(targetSecs)}
              </span>
            )}
            <span className="text-[#7E7B75] flex items-center gap-1 border-r border-[#E5E2DC] pr-2">
              <Eye className="w-3.5 h-3.5 text-[#D85631]" />
              <span>{t.showBar}</span>
            </span>
          </button>
        </div>
      )}

      {isBarVisible && (
        <div
          style={{ transform: `translate(${hudOffset.x}px, ${hudOffset.y}px)` }}
          className="self-center mb-6 flex flex-col items-center gap-2 transition-transform duration-75 animate-in fade-in duration-150"
        >
          <div className="relative flex items-center gap-3 bg-[#FFFFFF] border border-[#E5E2DC] rounded-2xl px-4 py-2.5 shadow-2xl select-none text-[#1E1D1B] overflow-hidden">
            {toggles.autoStopEnabled && targetSecs > 0 && (
              <div className="absolute bottom-0 left-0 right-0 h-1 bg-[#F4F3F0]">
                <div
                  className="h-full bg-[#D85631] transition-all duration-300"
                  style={{ width: `${progressPercent}%` }}
                />
              </div>
            )}

            <div
              onMouseDown={handleMouseDown}
              title={lang === 'en' ? 'Drag bar' : 'اسحب الشريط'}
              className="text-[#7E7B75] hover:text-[#1E1D1B] cursor-grab active:cursor-grabbing p-1"
            >
              <Move className="w-4 h-4" />
            </div>

            <div className="flex items-center gap-2 pr-1 border-r border-[#E5E2DC]">
              <span
                className={`w-2.5 h-2.5 rounded-full ${
                  status === 'recording' ? 'bg-[#D85631] animate-ping' : 'bg-amber-500'
                }`}
              />
              <span className="text-xs font-black font-mono text-[#D85631]">
                {status === 'recording' ? 'REC' : status === 'paused' ? 'PAUSED' : 'READY'}
              </span>
            </div>

            <div className="font-mono text-base font-black text-[#1E1D1B] tracking-wider tabular-nums px-2 flex items-center gap-1" dir="ltr">
              <span>{timerFormatted}</span>
              {toggles.autoStopEnabled && targetSecs > 0 && (
                <span className="text-[11px] font-bold text-[#7E7B75] font-mono">
                  / {formatTargetTime(targetSecs)}
                </span>
              )}
            </div>

            <div className="flex items-center gap-1.5 border-r border-[#E5E2DC] pr-2">
              <button
                type="button"
                onClick={onPauseResume}
                title={status === 'recording' ? t.pause : t.resume}
                className="p-1.5 rounded-xl text-[#1E1D1B] hover:bg-[#F4F3F0] transition-colors cursor-pointer"
              >
                {status === 'recording' ? (
                  <Pause className="w-4 h-4 fill-current text-amber-500" />
                ) : (
                  <Play className="w-4 h-4 fill-current text-emerald-600" />
                )}
              </button>

              <button
                type="button"
                onClick={onSave}
                title={t.stopRecording}
                className="flex items-center gap-1.5 px-3 py-1.5 bg-[#D85631] hover:bg-[#C24623] text-white text-xs font-bold rounded-xl shadow-sm transition-colors cursor-pointer whitespace-nowrap"
              >
                <Square className="w-3.5 h-3.5 fill-current" />
                <span>{t.stopRecording}</span>
              </button>

              <button
                type="button"
                onClick={onCancel}
                title={t.discard}
                className="flex items-center gap-1 px-2.5 py-1.5 bg-red-50 hover:bg-red-100 text-red-700 text-xs font-bold rounded-xl transition-colors cursor-pointer whitespace-nowrap"
              >
                <X className="w-3.5 h-3.5" />
                <span>{t.cancelRecording}</span>
              </button>

              <button
                type="button"
                onClick={handleHideBar}
                title={t.hideBar}
                className="p-1.5 rounded-xl text-[#7E7B75] hover:text-[#D85631] hover:bg-[#F4F3F0] transition-colors cursor-pointer"
              >
                <Trash2 className="w-4 h-4" />
              </button>

              <button
                type="button"
                onClick={onTakeSnapshot}
                title={lang === 'en' ? 'Snapshot' : 'لقطة شاشة'}
                className="p-1.5 rounded-xl text-[#7E7B75] hover:text-[#D85631] hover:bg-[#F4F3F0] transition-colors cursor-pointer"
              >
                <Camera className="w-4 h-4" />
              </button>
            </div>
          </div>

          <div className="flex items-center gap-1.5 bg-white/95 px-3 py-0.5 rounded-full border border-[#E5E2DC] text-[10px] font-bold text-[#7E7B75] shadow-sm">
            <EyeOff className="w-3 h-3 text-[#D85631]" />
            <span>
              {lang === 'en'
                ? 'Hidden from recorded video'
                : 'مخفي تلقائياً من الظهور داخل الفيديو المسجل'}
            </span>
          </div>
        </div>
      )}
    </div>
  );
};
