/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState, useEffect, useRef } from 'react';
import { Header } from './components/Header';
import { CaptureModeSelector } from './components/CaptureModeSelector';
import { QualitySettings } from './components/QualitySettings';
import { StorageAndNaming } from './components/StorageAndNaming';
import { SettingsModal } from './components/SettingsModal';
import { InteractiveRegionSelector } from './components/InteractiveRegionSelector';
import { LiveRecordingOverlay } from './components/LiveRecordingOverlay';
import {
  CaptureMode,
  AspectRatioPreset,
  RegionCoordinates,
  ResolutionOption,
  FpsOption,
  SharpnessMode,
  VideoFormat,
  ToggleSettings,
  StorageSettings,
} from './types/recorder';
import { Language, translations } from './utils/translations';
import { localDateStamp } from './utils/format';
import { desktopApi, isDesktopApp, UpdateCheckResult } from './utils/desktopApi';
import { CircleDot, CheckCircle2, Download } from 'lucide-react';

const SETTINGS_STORAGE_KEY = 'medialab_recorder_settings_v4';

export default function App() {
  const loadSavedSettings = () => {
    try {
      const saved = localStorage.getItem(SETTINGS_STORAGE_KEY);
      if (saved) return JSON.parse(saved);
    } catch {
      // ignore
    }
    return null;
  };

  const initialConfig = loadSavedSettings();
  const isDesktop = isDesktopApp();

  const [lang, setLang] = useState<Language>(initialConfig?.lang || 'en');
  const t = translations[lang];
  const tRef = useRef(t);
  tRef.current = t;
  const handleStartRef = useRef<(skipCountdown?: boolean) => void>(() => {});
  const toastTimerRef = useRef<any>(null);

  useEffect(() => {
    document.documentElement.dir = lang === 'ar' ? 'rtl' : 'ltr';
    document.documentElement.lang = lang;
  }, [lang]);

  const [isSettingsOpen, setIsSettingsOpen] = useState(false);

  const [appVersion, setAppVersion] = useState<string>('1.1.5');
  const [updateInfo, setUpdateInfo] = useState<UpdateCheckResult | null>(null);
  const [isCheckingUpdate, setIsCheckingUpdate] = useState(false);
  const [isInstallingUpdate, setIsInstallingUpdate] = useState(false);
  const [updateProgress, setUpdateProgress] = useState<number>(0);
  const [updateStatusMsg, setUpdateStatusMsg] = useState<string | null>(null);

  const [captureMode, setCaptureMode] = useState<CaptureMode>(
    initialConfig?.captureMode || 'fullscreen'
  );
  const [aspectPreset, setAspectPreset] = useState<AspectRatioPreset>(
    initialConfig?.aspectPreset || '16:9'
  );
  const [region, setRegion] = useState<RegionCoordinates>(
    initialConfig?.region || {
      x: 0,
      y: 0,
      width: 1280,
      height: 720,
    }
  );
  const [selectedWindowId, setSelectedWindowId] = useState<string>(
    initialConfig?.selectedWindowId || ''
  );

  const [resolution, setResolution] = useState<ResolutionOption>(
    initialConfig?.resolution || 'native'
  );
  const [fps, setFps] = useState<FpsOption>(initialConfig?.fps || 30);
  const [sharpness, setSharpness] = useState<SharpnessMode>(
    initialConfig?.sharpness || 'high'
  );
  const [videoFormat, setVideoFormat] = useState<VideoFormat>(
    initialConfig?.videoFormat || 'mp4'
  );

  const [toggles, setToggles] = useState<ToggleSettings>(() => {
    const defaults: ToggleSettings = {
      hideCursor: true,
      highlightClicks: false,
      systemAudio: true,
      micAudio: false,
      countdown3s: false,
      countdownDuration: 3,
      autoStopEnabled: false,
      autoStopHours: 0,
      autoStopMinutes: 5,
      autoStopSeconds: 0,
    };
    return {
      ...defaults,
      ...(initialConfig?.toggles || {}),
    };
  });

  const [storage, setStorage] = useState<StorageSettings>(
    initialConfig?.storage || {
      savePath: '',
      autoSequential: true,
      startIndex: 0,
      currentIndex: 0,
      prefix: '',
      format: 'mp4',
    }
  );
  const [nativePreferencesLoaded, setNativePreferencesLoaded] = useState(!isDesktop);

  useEffect(() => {
    setStorage((prev) => ({ ...prev, format: videoFormat }));
  }, [videoFormat]);

  useEffect(() => {
    if (!nativePreferencesLoaded || !storage.savePath || !isDesktop) return;
    void desktopApi.savePreferences({ savePath: storage.savePath }).catch(() => {});
  }, [storage.savePath, nativePreferencesLoaded, isDesktop]);

  useEffect(() => {
    try {
      const configToSave = {
        lang,
        captureMode,
        aspectPreset,
        region,
        selectedWindowId,
        resolution,
        fps,
        sharpness,
        videoFormat,
        toggles,
        storage,
      };
      localStorage.setItem(SETTINGS_STORAGE_KEY, JSON.stringify(configToSave));
    } catch {
      // ignore
    }
  }, [
    lang,
    captureMode,
    aspectPreset,
    region,
    selectedWindowId,
    resolution,
    fps,
    sharpness,
    videoFormat,
    toggles,
    storage,
  ]);

  const handleCheckUpdates = async (silent = false) => {
    if (!isDesktop || isCheckingUpdate) return;
    setIsCheckingUpdate(true);
    if (!silent) setUpdateStatusMsg(null);
    try {
      const result = await desktopApi.checkForUpdates();
      setUpdateInfo(result);
      if (!silent) {
        setUpdateStatusMsg(result.available ? `${tRef.current.updateAvailable}: v${result.version}` : tRef.current.upToDate);
      }
    } catch {
      if (!silent) setUpdateStatusMsg(tRef.current.updateError);
    } finally {
      setIsCheckingUpdate(false);
    }
  };

  const handleInstallUpdate = async () => {
    if (!updateInfo?.updateObj || isInstallingUpdate) return;
    setIsInstallingUpdate(true);
    setUpdateProgress(0);
    try {
      await desktopApi.downloadAndInstallUpdate(updateInfo.updateObj, (pct) => {
        setUpdateProgress(pct);
      });
    } catch {
      setIsInstallingUpdate(false);
      showToast(tRef.current.updateError);
    }
  };

  useEffect(() => {
    if (!isDesktop) return;

    void desktopApi.getAppVersion().then(setAppVersion).catch(() => {});
    void handleCheckUpdates(true);

    desktopApi
      .getDefaultSaveInfo(initialConfig?.storage?.savePath, initialConfig?.storage?.prefix || '')
      .then((info) => {
        if (info?.savePath) {
          setStorage((prev) => ({
            ...prev,
            savePath: info.savePath,
            currentIndex: info.currentIndex ?? prev.currentIndex,
          }));
        }
      })
      .catch(() => {})
      .finally(() => setNativePreferencesLoaded(true));

    const offStatus = desktopApi.onRecordingStatusChange((event) => {
      if (event?.status === 'idle') {
        setRecordingStatus('idle');
        setRecordingSeconds(0);
      } else if (event?.status === 'recording' || event?.status === 'paused' || event?.status === 'saving') {
        setRecordingStatus(event.status);
      }
    });
    const offSaved = desktopApi.onRecordingSaved((result) => {
      if (result?.currentIndex != null) setStorage((prev) => ({ ...prev, currentIndex: result.currentIndex }));
      showToast(`${tRef.current.toastSaved}: ${result?.fileName || ''}`);
    });
    const offCancelled = desktopApi.onRecordingCancelled(() => {
      setRecordingStatus('idle');
      setRecordingSeconds(0);
      showToast(tRef.current.toastDiscarded);
    });
    const offError = desktopApi.onRecordingError((message) => showToast(message));
    const offHotkey = desktopApi.onHotkeyStart(() => handleStartRef.current(true));

    return () => {
      offStatus();
      offSaved();
      offCancelled();
      offError();
      offHotkey();
    };
  }, []);

  const [isRegionPickerOpen, setIsRegionPickerOpen] = useState(false);

  const [recordingStatus, setRecordingStatus] = useState<
    'idle' | 'countdown' | 'recording' | 'paused' | 'saving'
  >('idle');
  const [recordingSeconds, setRecordingSeconds] = useState(0);
  const [countdownValue, setCountdownValue] = useState(3);
  const [toastMessage, setToastMessage] = useState<string | null>(null);

  const countdownTimerRef = useRef<any>(null);
  const recordIntervalRef = useRef<any>(null);

  const formatTimer = (totalSecs: number) => {
    const hrs = Math.floor(totalSecs / 3600);
    const mins = Math.floor((totalSecs % 3600) / 60);
    const secs = totalSecs % 60;
    return `${hrs.toString().padStart(2, '0')}:${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  };

  const showToast = (msg: string) => {
    setToastMessage(msg);
    if (toastTimerRef.current) clearTimeout(toastTimerRef.current);
    toastTimerRef.current = setTimeout(() => {
      setToastMessage(null);
    }, 4500);
  };

  const startNativeRecording = async () => {
    if (!isDesktop) return;
    try {
      await desktopApi.startRecord({
        mode: captureMode,
        region,
        windowTitle: selectedWindowId,
        resolution,
        fps,
        sharpness,
        hideCursor: toggles.hideCursor,
        systemAudio: toggles.systemAudio,
        micAudio: toggles.micAudio,
        format: storage.format,
        savePath: storage.savePath,
        currentIndex: storage.currentIndex,
        prefix: storage.prefix.trim(),
        autoSequential: storage.autoSequential,
        lang,
      });
    } catch (error) {
      console.error(error);
      showToast(typeof error === 'string' ? error : error instanceof Error ? error.message : 'Could not start recording');
      setRecordingStatus('idle');
    }
  };

  const handleStartRecording = (skipCountdown = false) => {
    if (recordingStatus !== 'idle') return;

    if (toggles.countdown3s && !skipCountdown) {
      setRecordingStatus('countdown');
      setCountdownValue(toggles.countdownDuration);
      if (countdownTimerRef.current) clearInterval(countdownTimerRef.current);

      let currentCount = toggles.countdownDuration;
      countdownTimerRef.current = setInterval(() => {
        currentCount -= 1;
        if (currentCount <= 0) {
          clearInterval(countdownTimerRef.current);
          setRecordingStatus('recording');
          setRecordingSeconds(0);
          void startNativeRecording();
        } else {
          setCountdownValue(currentCount);
        }
      }, 1000);
    } else {
      setRecordingStatus('recording');
      setRecordingSeconds(0);
      void startNativeRecording();
    }
  };

  const handlePauseResume = () => {
    if (recordingStatus === 'recording') {
      setRecordingStatus('paused');
      if (isDesktop) void desktopApi.pauseRecord();
    } else if (recordingStatus === 'paused') {
      setRecordingStatus('recording');
      if (isDesktop) void desktopApi.resumeRecord();
    }
  };

  const stopInProgressRef = useRef<Promise<any> | null>(null);

  const handleCancelRecording = async () => {
    if (stopInProgressRef.current) return stopInProgressRef.current;
    const cancelTask = (async () => {
      if (countdownTimerRef.current) clearInterval(countdownTimerRef.current);
      if (recordIntervalRef.current) clearInterval(recordIntervalRef.current);
      if (recordingStatus === 'countdown') {
        setRecordingStatus('idle');
        setRecordingSeconds(0);
        showToast(t.countdownCancelled);
        return;
      }
      if (isDesktop) {
        try {
          await desktopApi.cancelRecord();
        } catch (error) {
          showToast(typeof error === 'string' ? error : error instanceof Error ? error.message : 'Could not cancel recording');
        }
      } else {
        setRecordingStatus('idle');
        setRecordingSeconds(0);
        showToast(t.toastDiscarded);
      }
    })();
    stopInProgressRef.current = cancelTask;
    try { await cancelTask; }
    finally { stopInProgressRef.current = null; }
  };

  const handleStopAndSave = async () => {
    if (stopInProgressRef.current) return stopInProgressRef.current;
    const stopTask = (async () => {
      if (recordingStatus === 'countdown') {
        if (countdownTimerRef.current) clearInterval(countdownTimerRef.current);
        setRecordingStatus('idle');
        setRecordingSeconds(0);
        showToast(t.countdownCancelled);
        return;
      }

      // Freeze timer immediately while FFmpeg finalizes the video
      if (recordIntervalRef.current) clearInterval(recordIntervalRef.current);
      setRecordingStatus('saving');

      let nativeResult: any = null;
      if (isDesktop) {
        try {
          nativeResult = await desktopApi.stopSave();
          if (!nativeResult?.path) {
            setRecordingStatus('idle');
            setRecordingSeconds(0);
            showToast(lang === 'en' ? 'Recording was not saved. Please try again.' : 'لم يتم حفظ التسجيل. حاول مرة أخرى.');
            return;
          }
        } catch (error) {
          setRecordingStatus('idle');
          setRecordingSeconds(0);
          showToast(typeof error === 'string' ? error : error instanceof Error ? error.message : 'Could not save recording');
          return;
        }
      }

      setRecordingStatus('idle');
      setRecordingSeconds(0);
      if (recordIntervalRef.current) clearInterval(recordIntervalRef.current);
      if (countdownTimerRef.current) clearInterval(countdownTimerRef.current);

      if (!nativeResult) {
        if (storage.autoSequential) {
          setStorage((prev) => ({ ...prev, currentIndex: prev.currentIndex + 1 }));
        }
        const previewName = storage.autoSequential
          ? `${storage.prefix.trim()}${storage.currentIndex}.${storage.format}`
          : `recording_${localDateStamp()}.${storage.format}`;
        showToast(`${t.toastSaved}: ${previewName}`);
      }
    })();
    stopInProgressRef.current = stopTask;
    try { await stopTask; }
    finally { stopInProgressRef.current = null; }
  };

  const handleStopAndSaveRef = useRef(handleStopAndSave);
  handleStopAndSaveRef.current = handleStopAndSave;
  handleStartRef.current = handleStartRecording;

  useEffect(() => {
    if (recordingStatus !== 'recording') return;
    recordIntervalRef.current = setInterval(() => {
      setRecordingSeconds((prev) => prev + 1);
    }, 1000);
    return () => {
      if (recordIntervalRef.current) clearInterval(recordIntervalRef.current);
    };
  }, [recordingStatus]);

  useEffect(() => {
    if (recordingStatus !== 'recording' || !toggles.autoStopEnabled) return;
    const targetSecs =
      (toggles.autoStopHours || 0) * 3600 +
      (toggles.autoStopMinutes || 0) * 60 +
      (toggles.autoStopSeconds || 0);
    if (targetSecs > 0 && recordingSeconds >= targetSecs && !stopInProgressRef.current) {
      showToast(`${t.autoStopToast} ${formatTimer(targetSecs)}`);
      void handleStopAndSaveRef.current();
    }
  }, [
    recordingSeconds,
    recordingStatus,
    toggles.autoStopEnabled,
    toggles.autoStopHours,
    toggles.autoStopMinutes,
    toggles.autoStopSeconds,
  ]);

  const handleTakeSnapshot = () => {
    showToast(t.snapshotUnavailable);
  };

  return (
    <div className="min-h-screen w-full bg-[#F4F3F0] text-[#1E1D1B] flex flex-col font-sans selection:bg-[#D85631]/20 selection:text-[#D85631]">
      <Header
        lang={lang}
        setLang={setLang}
        onOpenSettings={() => setIsSettingsOpen(true)}
        recordingStatus={recordingStatus}
        recordingTimer={formatTimer(recordingSeconds)}
        onStopRecording={() => handleStopAndSave()}
        onCancelRecording={() => handleCancelRecording()}
        onPauseResumeRecording={handlePauseResume}
        appVersion={appVersion}
      />

      {updateInfo?.available && (
        <div className="max-w-xl w-full mx-auto px-5 pt-3">
          <div className="bg-[#FFF4F0] border border-[#D85631]/40 rounded-xl px-3.5 py-2.5 flex items-center justify-between gap-3 text-xs">
            <div className="flex items-center gap-2 font-bold text-[#1E1D1B]">
              <Download className="w-4 h-4 text-[#D85631] shrink-0" />
              <span>
                {t.updateAvailable} <span className="font-mono text-[#D85631]" dir="ltr">v{updateInfo.version}</span>
              </span>
            </div>
            <button
              type="button"
              disabled={isInstallingUpdate}
              onClick={() => void handleInstallUpdate()}
              className="px-3 py-1.5 bg-[#D85631] hover:bg-[#C24623] disabled:opacity-60 text-white font-bold rounded-lg cursor-pointer whitespace-nowrap"
            >
              {isInstallingUpdate ? `${t.downloadingUpdate} ${updateProgress}%` : t.installUpdate}
            </button>
          </div>
        </div>
      )}

      <main className="flex-1 w-full max-w-xl mx-auto px-5 py-5 space-y-4 box-border overflow-x-hidden">
        <CaptureModeSelector
          lang={lang}
          captureMode={captureMode}
          setCaptureMode={setCaptureMode}
          aspectPreset={aspectPreset}
          setAspectPreset={setAspectPreset}
          region={region}
          setRegion={setRegion}
          onOpenRegionPickerModal={async () => {
            if (isDesktop) {
              try {
                const selected = await desktopApi.selectRegion(lang);
                if (selected && !selected.cancelled && selected.width && selected.height) {
                  setRegion({
                    x: selected.x || 0,
                    y: selected.y || 0,
                    width: selected.width,
                    height: selected.height,
                  });
                }
              } catch (error) {
                showToast(typeof error === 'string' ? error : error instanceof Error ? error.message : 'Region selection failed');
              }
            } else {
              setIsRegionPickerOpen(true);
            }
          }}
          selectedWindowId={selectedWindowId}
          setSelectedWindowId={setSelectedWindowId}
        />

        <QualitySettings
          lang={lang}
          resolution={resolution}
          setResolution={setResolution}
          fps={fps}
          setFps={setFps}
        />

        <StorageAndNaming
          lang={lang}
          storage={storage}
          setStorage={setStorage}
        />

        <div className="pt-2 w-full min-w-0">
          <button
            type="button"
            disabled={recordingStatus === 'saving'}
            onClick={() => handleStartRecording()}
            className="w-full py-3.5 px-6 bg-[#D85631] hover:bg-[#C24623] disabled:opacity-60 active:scale-[0.99] text-white text-base font-bold rounded-xl shadow-sm transition-all duration-150 flex items-center justify-center gap-2.5 cursor-pointer truncate"
          >
            <CircleDot className="w-5 h-5 fill-current shrink-0" />
            <span className="truncate">
              {recordingStatus === 'saving' ? t.savingRecording : t.startRecording}
            </span>
          </button>
        </div>
      </main>

      {toastMessage && (
        <div className="fixed bottom-6 left-1/2 -translate-x-1/2 z-50 bg-[#1E1D1B] text-white px-5 py-2.5 rounded-xl shadow-xl flex items-center gap-2.5 text-xs font-bold animate-in slide-in-from-bottom-3 duration-150 max-w-[90vw]">
          <CheckCircle2 className="w-4 h-4 text-[#D85631] shrink-0" />
          <span className="break-words">{toastMessage}</span>
        </div>
      )}

      <SettingsModal
        lang={lang}
        isOpen={isSettingsOpen}
        onClose={() => setIsSettingsOpen(false)}
        toggles={toggles}
        setToggles={setToggles}
        storage={storage}
        setStorage={setStorage}
        sharpness={sharpness}
        setSharpness={setSharpness}
        videoFormat={videoFormat}
        setVideoFormat={setVideoFormat}
        appVersion={appVersion}
        updateInfo={updateInfo}
        isCheckingUpdate={isCheckingUpdate}
        isInstallingUpdate={isInstallingUpdate}
        updateProgress={updateProgress}
        updateStatusMsg={updateStatusMsg}
        onCheckUpdates={() => void handleCheckUpdates(false)}
        onInstallUpdate={() => void handleInstallUpdate()}
      />

      <InteractiveRegionSelector
        isOpen={isRegionPickerOpen}
        onClose={() => setIsRegionPickerOpen(false)}
        currentRegion={region}
        onApplyRegion={(newRegion) => {
          setRegion(newRegion);
          showToast(
            lang === 'en'
              ? `Region set to ${newRegion.width} × ${newRegion.height}`
              : `تم تحديد الأبعاد: ${newRegion.width} × ${newRegion.height}`
          );
        }}
      />

      {(!isDesktop || recordingStatus === 'countdown') && recordingStatus !== 'saving' && (
        <LiveRecordingOverlay
          lang={lang}
          isActive={recordingStatus !== 'idle'}
          status={recordingStatus}
          timerFormatted={formatTimer(recordingSeconds)}
          secondsElapsed={recordingSeconds}
          countdownValue={countdownValue}
          storage={storage}
          toggles={toggles}
          region={region}
          captureMode={captureMode}
          onPauseResume={handlePauseResume}
          onSave={() => handleStopAndSave()}
          onCancel={() => handleCancelRecording()}
          onTakeSnapshot={handleTakeSnapshot}
          onShowToast={showToast}
        />
      )}
    </div>
  );
}
