import React, { useRef } from 'react';
import { StorageSettings } from '../types/recorder';
import { Language, translations } from '../utils/translations';
import { Hash, FolderOpen } from 'lucide-react';
import { localDateStamp } from '../utils/format';
import { desktopApi, isDesktopApp } from '../utils/desktopApi';

interface StorageAndNamingProps {
  lang: Language;
  storage: StorageSettings;
  setStorage: React.Dispatch<React.SetStateAction<StorageSettings>>;
}

export const StorageAndNaming: React.FC<StorageAndNamingProps> = ({
  lang,
  storage,
  setStorage,
}) => {
  const t = translations[lang];
  const fileInputRef = useRef<HTMLInputElement>(null);

  const upcomingFileName = storage.autoSequential
    ? `${storage.prefix.trim()}${storage.currentIndex}.${storage.format}`
    : `recording_${localDateStamp()}.${storage.format}`;

  const handleBrowseClick = async () => {
    if (isDesktopApp()) {
      try {
        const result = await desktopApi.chooseFolder(storage.prefix);
        if (result) {
          setStorage((prev) => ({
            ...prev,
            savePath: result.savePath,
            currentIndex: result.currentIndex ?? prev.currentIndex,
          }));
        }
      } catch (error) {
        console.error('Folder selection failed', error);
      }
      return;
    }
    if (fileInputRef.current) fileInputRef.current.click();
  };

  const handleDirectoryChange = (e: React.ChangeEvent<HTMLInputElement>) => {
    const files = e.target.files;
    if (files && files.length > 0) {
      // @ts-ignore
      const dirName = files[0].webkitRelativePath?.split('/')[0] || files[0].name;
      setStorage((prev) => ({
        ...prev,
        savePath: `C:\\Users\\User\\Videos\\${dirName}`,
      }));
    }
    if (fileInputRef.current) {
      fileInputRef.current.value = '';
    }
  };

  const handleToggleSequential = () => {
    setStorage((prev) => ({
      ...prev,
      autoSequential: !prev.autoSequential,
    }));
  };

  return (
    <div className="space-y-2.5 w-full min-w-0">
      <input
        ref={fileInputRef}
        type="file"
        // @ts-ignore
        webkitdirectory="true"
        directory="true"
        className="hidden"
        onChange={handleDirectoryChange}
      />

      <div className="flex items-center justify-between text-xs">
        <label className="font-bold text-[#7E7B75] uppercase tracking-wider">
          {t.saveTo}
        </label>

        <div className="flex items-center gap-2">
          <button
            type="button"
            onClick={handleToggleSequential}
            className={`flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-[11px] font-bold border transition-colors cursor-pointer ${
              storage.autoSequential
                ? 'bg-[#D85631] text-white border-[#D85631] shadow-sm'
                : 'bg-[#FFFFFF] text-[#7E7B75] border-[#E5E2DC] hover:text-[#1E1D1B]'
            }`}
            title={t.sequentialNumbering}
          >
            <Hash className="w-3 h-3" />
            <span>{storage.autoSequential ? t.sequentialOn : t.sequentialOff}</span>
          </button>

          <span className="font-mono font-bold text-[#D85631] text-[11px] truncate" dir="ltr">
            {upcomingFileName}
          </span>
        </div>
      </div>

      <div className="flex items-center gap-2 w-full min-w-0">
        <input
          type="text"
          dir="ltr"
          value={storage.savePath}
          onChange={(e) =>
            setStorage((prev) => ({ ...prev, savePath: e.target.value }))
          }
          onBlur={() => {
            if (storage.savePath.trim() && isDesktopApp()) {
              void desktopApi.savePreferences({ savePath: storage.savePath, createSavePath: true }).catch(() => {});
            }
          }}
          placeholder="C:\Users\Videos\Captures"
          className="flex-1 min-w-0 bg-[#FFFFFF] border border-[#E5E2DC] focus:border-[#D85631] rounded-xl px-3.5 py-3 text-xs font-mono text-[#1E1D1B] outline-none shadow-sm truncate"
        />

        <button
          type="button"
          onClick={handleBrowseClick}
          className="px-4 sm:px-5 py-3 bg-[#FFFFFF] hover:bg-[#F9F8F6] text-[#1E1D1B] text-xs font-bold rounded-xl border border-[#E5E2DC] transition-colors cursor-pointer whitespace-nowrap shadow-sm shrink-0 flex items-center gap-1.5"
        >
          <FolderOpen className="w-4 h-4 text-[#D85631]" />
          <span>{t.browse}</span>
        </button>
      </div>
    </div>
  );
};
