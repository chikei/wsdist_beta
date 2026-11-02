# Match Rust's Windows FileType classification without following name-surrogate
# reparse points. Non-link reparses, such as deduplicated files, remain regular.
if (-not ('Wsdist.Packaging.WindowsFileType' -as [type])) {
    Add-Type -TypeDefinition @'
using System;
using System.ComponentModel;
using System.IO;
using System.Runtime.InteropServices;
using Microsoft.Win32.SafeHandles;

namespace Wsdist.Packaging {
    public readonly struct WindowsEntryType {
        public bool IsSymlink { get; }
        public bool IsDirectory { get; }
        public bool IsFile { get; }

        internal WindowsEntryType(uint attributes, uint reparseTag) {
            IsSymlink = (attributes & 0x400u) != 0 && (reparseTag & 0x20000000u) != 0;
            IsDirectory = !IsSymlink && (attributes & 0x10u) != 0;
            IsFile = !IsSymlink && !IsDirectory;
        }
    }

    public static class WindowsFileType {
        [StructLayout(LayoutKind.Sequential)]
        private struct FileAttributeTagInfo {
            public uint FileAttributes;
            public uint ReparseTag;
        }

        [DllImport("kernel32.dll", EntryPoint = "CreateFileW", CharSet = CharSet.Unicode,
            ExactSpelling = true, SetLastError = true)]
        private static extern SafeFileHandle CreateFile(string path, uint desiredAccess,
            uint shareMode, IntPtr securityAttributes, uint creationDisposition,
            uint flagsAndAttributes, IntPtr templateFile);

        [DllImport("kernel32.dll", ExactSpelling = true, SetLastError = true)]
        [return: MarshalAs(UnmanagedType.Bool)]
        private static extern bool GetFileInformationByHandleEx(SafeFileHandle handle,
            int informationClass, out FileAttributeTagInfo information, uint bufferSize);

        public static WindowsEntryType Read(FileSystemInfo item) {
            uint attributes = (uint)item.Attributes;
            if ((attributes & 0x400u) == 0) {
                return new WindowsEntryType(attributes, 0);
            }

            string path = Path.GetFullPath(item.FullName);
            if (!path.StartsWith(@"\\?\", StringComparison.Ordinal)) {
                path = path.StartsWith(@"\\", StringComparison.Ordinal)
                    ? @"\\?\UNC\" + path.Substring(2)
                    : @"\\?\" + path;
            }
            // No data access; share read/write/delete and open the reparse entry
            // itself. BACKUP_SEMANTICS also permits directory metadata handles.
            using (SafeFileHandle handle = CreateFile(path, 0, 0x7u, IntPtr.Zero,
                3u, 0x02000000u | 0x00200000u, IntPtr.Zero)) {
                if (handle.IsInvalid) {
                    throw new Win32Exception(Marshal.GetLastWin32Error(),
                        "Cannot open Windows entry metadata: " + item.FullName);
                }
                FileAttributeTagInfo information;
                if (!GetFileInformationByHandleEx(handle, 9, out information, 8)) {
                    throw new Win32Exception(Marshal.GetLastWin32Error(),
                        "Cannot read Windows reparse metadata: " + item.FullName);
                }
                return new WindowsEntryType(information.FileAttributes, information.ReparseTag);
            }
        }
    }
}
'@
}

function Get-WsdistFileType([IO.FileSystemInfo] $Item) {
    [Wsdist.Packaging.WindowsFileType]::Read($Item)
}
