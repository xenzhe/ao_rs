# Runs a program on a hidden Windows desktop (never shown on the user's screen, cannot take
# the focus) and waits for it. For automated game screenshots while someone is playing:
#   powershell -File scripts\captura_invisible.ps1 -Exe target\debug\client.exe [-TimeoutSec 60]
# Environment variables (AO_SHOT, AO_SERVER, AO_OPEN...) are inherited by the program.
param(
    [Parameter(Mandatory = $true)][string]$Exe,
    [string]$Arguments = "",
    [int]$TimeoutSec = 90
)
$code = @"
using System;
using System.Runtime.InteropServices;
public static class HiddenDesktop {
    [StructLayout(LayoutKind.Sequential, CharSet = CharSet.Unicode)]
    public struct STARTUPINFO {
        public int cb; public string lpReserved; public string lpDesktop; public string lpTitle;
        public int dwX, dwY, dwXSize, dwYSize, dwXCountChars, dwYCountChars, dwFillAttribute, dwFlags;
        public short wShowWindow, cbReserved2; public IntPtr lpReserved2, hStdInput, hStdOutput, hStdError;
    }
    [StructLayout(LayoutKind.Sequential)]
    public struct PROCESS_INFORMATION { public IntPtr hProcess, hThread; public int dwProcessId, dwThreadId; }
    [DllImport("user32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern IntPtr CreateDesktopW(string name, IntPtr dev, IntPtr mode, int flags, uint access, IntPtr sa);
    [DllImport("user32.dll")] static extern bool CloseDesktop(IntPtr h);
    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    static extern bool CreateProcessW(string app, string cmd, IntPtr pa, IntPtr ta, bool inherit, uint flags, IntPtr env, string dir, ref STARTUPINFO si, out PROCESS_INFORMATION pi);
    [DllImport("kernel32.dll")] static extern uint WaitForSingleObject(IntPtr h, uint ms);
    [DllImport("kernel32.dll")] static extern bool TerminateProcess(IntPtr h, uint code);
    [DllImport("kernel32.dll")] static extern bool GetExitCodeProcess(IntPtr h, out uint code);
    [DllImport("kernel32.dll")] static extern bool CloseHandle(IntPtr h);
    public static int Run(string exe, string args, string dir, int timeoutMs) {
        IntPtr desk = CreateDesktopW("ao_capture", IntPtr.Zero, IntPtr.Zero, 0, 0x10000000, IntPtr.Zero);
        if (desk == IntPtr.Zero) return -2;
        var si = new STARTUPINFO(); si.cb = Marshal.SizeOf(si); si.lpDesktop = "ao_capture";
        PROCESS_INFORMATION pi;
        string cmd = "\"" + exe + "\" " + args;
        if (!CreateProcessW(null, cmd, IntPtr.Zero, IntPtr.Zero, false, 0, IntPtr.Zero, dir, ref si, out pi)) { CloseDesktop(desk); return -3; }
        uint r = WaitForSingleObject(pi.hProcess, (uint)timeoutMs);
        if (r != 0) TerminateProcess(pi.hProcess, 1);
        uint ec; GetExitCodeProcess(pi.hProcess, out ec);
        CloseHandle(pi.hThread); CloseHandle(pi.hProcess); CloseDesktop(desk);
        return r == 0 ? (int)ec : -1;
    }
}
"@
Add-Type -TypeDefinition $code
$full = (Resolve-Path $Exe).Path
$rc = [HiddenDesktop]::Run($full, $Arguments, (Get-Location).Path, $TimeoutSec * 1000)
exit $rc
