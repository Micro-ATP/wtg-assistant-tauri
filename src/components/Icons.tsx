import React from 'react'

interface IconProps {
  size?: number
  color?: string
  className?: string
}

// Home Icon
export const HomeIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <path d="M3 9l9-7 9 7v11a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" />
    <polyline points="9 22 9 12 15 12 15 22" />
  </svg>
)

// Settings Icon
export const SettingsIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <circle cx="12" cy="12" r="3" />
    <path d="M12 1v6m0 6v6m4.22-15.22l-4.24 4.24m-5.96 5.96l-4.24 4.24m15.96-4.24l4.24 4.24m-4.24-15.96l4.24-4.24" />
  </svg>
)

// Write/Disk Icon
export const WriteIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8z" />
    <path d="M12 6v6l5 3" />
  </svg>
)

// Benchmark Icon
export const BenchmarkIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <polyline points="12 3 20 7.5 20 16.5 12 21 4 16.5 4 7.5 12 3" />
    <polyline points="12 12 20 7.5" />
    <polyline points="12 21 20 16.5" />
    <polyline points="12 12 4 7.5" />
    <polyline points="12 12 12 21" />
  </svg>
)

// Tools Icon
export const ToolsIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <path d="M14.7 6.3a4 4 0 0 0-5.4 5.4L3 18l3 3 6.3-6.3a4 4 0 0 0 5.4-5.4l-2.1 2.1-3-3 2.1-2.1z" />
  </svg>
)

// Menu Icon
export const MenuIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <line x1="3" y1="6" x2="21" y2="6" />
    <line x1="3" y1="12" x2="21" y2="12" />
    <line x1="3" y1="18" x2="21" y2="18" />
  </svg>
)

// Close Icon
export const CloseIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <line x1="18" y1="6" x2="6" y2="18" />
    <line x1="6" y1="6" x2="18" y2="18" />
  </svg>
)

// Sun Icon
export const SunIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <circle cx="12" cy="12" r="5" />
    <line x1="12" y1="1" x2="12" y2="3" />
    <line x1="12" y1="21" x2="12" y2="23" />
    <line x1="4.22" y1="4.22" x2="5.64" y2="5.64" />
    <line x1="18.36" y1="18.36" x2="19.78" y2="19.78" />
    <line x1="1" y1="12" x2="3" y2="12" />
    <line x1="21" y1="12" x2="23" y2="12" />
    <line x1="4.22" y1="19.78" x2="5.64" y2="18.36" />
    <line x1="18.36" y1="5.64" x2="19.78" y2="4.22" />
  </svg>
)

// Moon Icon
export const MoonIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <path d="M21 12.79A9 9 0 1 1 11.21 3 7 7 0 0 0 21 12.79z" />
  </svg>
)

// Disk Icon
export const DiskIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <circle cx="12" cy="12" r="1" />
    <path d="M12 1C6.48 1 2 5.48 2 11v2c0 5.52 4.48 10 10 10h2c5.52 0 10-4.48 10-10v-2c0-5.52-4.48-10-10-10z" />
    <path d="M2.5 6.5h19M2.5 17.5h19" />
  </svg>
)

// Loading/Spinner Icon
export const SpinnerIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={`${className} animate-spin`}>
    <path d="M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2m0 18c-4.41 0-8-3.59-8-8s3.59-8 8-8 8 3.59 8 8-3.59 8-8 8" opacity="0.3" />
    <path d="M20.49 9c-1.04-4.18-4.93-7-9.49-7C6.48 2 2 6.48 2 12" strokeDasharray="30" />
  </svg>
)

// Window Icon (for app logo)
export const WindowIcon: React.FC<IconProps> = ({ size = 32, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="1.5" strokeLinecap="round" className={className}>
    <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
    <path d="M3 9h18" />
    <circle cx="7" cy="6" r="0.5" fill={color} />
    <circle cx="11" cy="6" r="0.5" fill={color} />
    <circle cx="15" cy="6" r="0.5" fill={color} />
  </svg>
)

// Refresh Icon
export const RefreshIcon: React.FC<IconProps> = ({ size = 24, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <polyline points="23 4 23 10 17 10" />
    <polyline points="1 20 1 14 7 14" />
    <path d="M3.51 9a9 9 0 0 1 14.85-3.36M20.49 15a9 9 0 0 1-14.85 3.36" />
  </svg>
)

// Folder Icon
export const FolderIcon: React.FC<IconProps> = ({ size = 20, color = 'currentColor', className }) => (
  <svg viewBox="0 0 24 24" width={size} height={size} fill="none" stroke={color} strokeWidth="2" strokeLinecap="round" className={className}>
    <path d="M3 7h5l2 3h11v8a2 2 0 0 1-2 2H4a1 1 0 0 1-1-1V7z" />
    <path d="M3 7V6a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v2" />
  </svg>
)
