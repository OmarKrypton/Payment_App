import React from "react";

type IconProps = { size?: number; color?: string; style?: React.CSSProperties };

const defaults: IconProps = { size: 14, color: "currentColor" };

export function IconSave({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M5 4h11l3 3v13H5z"/>
    <path d="M8 4v5h7V4"/>
    <path d="M8 20v-6h8v6"/>
  </svg>;
}

export function IconHistory({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <circle cx="12" cy="12" r="10"/>
    <polyline points="12 6 12 12 16 14"/>
  </svg>;
}

export function IconNewSession({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
    <polyline points="14 2 14 8 20 8"/>
    <line x1="12" y1="18" x2="12" y2="12"/>
    <line x1="9" y1="15" x2="15" y2="15"/>
  </svg>;
}

export function IconImport({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M21 15v4a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2v-4"/>
    <polyline points="7 10 12 15 17 10"/>
    <line x1="12" y1="15" x2="12" y2="3"/>
  </svg>;
}

export function IconExport({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M14 4h6v6"/>
    <path d="M20 4l-8 8"/>
    <path d="M18 14v5a1 1 0 0 1-1 1H5a1 1 0 0 1-1-1V7a1 1 0 0 1 1-1h5"/>
  </svg>;
}

export function IconChevronDown({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <polyline points="6 9 12 15 18 9"/>
  </svg>;
}

export function IconReport({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/>
    <polyline points="14 2 14 8 20 8"/>
    <line x1="16" y1="13" x2="8" y2="13"/>
    <line x1="16" y1="17" x2="8" y2="17"/>
    <polyline points="10 9 9 9 8 9"/>
  </svg>;
}

export function IconInvoice({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <rect x="3" y="3" width="18" height="18" rx="2" ry="2"/>
    <line x1="3" y1="9" x2="21" y2="9"/>
    <line x1="9" y1="21" x2="9" y2="9"/>
  </svg>;
}

// ── Concept sidebar icon set ──
export function IconBank({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M3 10 12 4l9 6"/>
    <path d="M5 10v9h14v-9"/>
    <path d="M9 19v-5h6v5"/>
  </svg>;
}

export function IconTruck({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M3 7h11v9H3z"/>
    <path d="M14 10h4l3 3v3h-7z"/>
    <circle cx="6.5" cy="17.5" r="1.5"/>
    <circle cx="17.5" cy="17.5" r="1.5"/>
  </svg>;
}

export function IconUsers({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M16 19v-1.5a3.5 3.5 0 0 0-3.5-3.5h-5A3.5 3.5 0 0 0 4 17.5V19"/>
    <circle cx="10" cy="7.5" r="3.5"/>
    <path d="M20 19v-1.5a3.5 3.5 0 0 0-2.6-3.4"/>
    <path d="M15.5 4.1a3.5 3.5 0 0 1 0 6.8"/>
  </svg>;
}

export function IconLayers({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="m12 3 9 5-9 5-9-5 9-5Z"/>
    <path d="M3 13l9 5 9-5"/>
    <path d="M3 17l9 5 9-5"/>
  </svg>;
}

export function IconUpload({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M12 16V4"/>
    <path d="m7 9 5-5 5 5"/>
    <path d="M4 20h16"/>
  </svg>;
}

export function IconExternal({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="M13 5h6v6"/>
    <path d="M19 5l-7 7"/>
    <path d="M11 5H7a2 2 0 0 0-2 2v10a2 2 0 0 0 2 2h10a2 2 0 0 0 2-2v-4"/>
  </svg>;
}

export function IconChevronRight({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <path d="m9 6 6 6-6 6"/>
  </svg>;
}

export function IconSettings({ size = defaults.size, color = defaults.color, style }: IconProps) {
  return <svg width={size} height={size} viewBox="0 0 24 24" fill="none" stroke={color} strokeWidth="1.9" strokeLinecap="round" strokeLinejoin="round" style={style}>
    <circle cx="12" cy="12" r="3"/>
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 0 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 0 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06A1.65 1.65 0 0 0 9 4.6h.09A1.65 1.65 0 0 0 10 3.09V3a2 2 0 0 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 0 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z"/>
  </svg>;
}
