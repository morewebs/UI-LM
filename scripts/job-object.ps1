# A job object whose members die when this process does.
#
# `check-everything` starts one powershell per check. When a run was interrupted, those children survived it: eleven of
# them were found burning five CPU-hours each, and every later measurement - including "the suite takes thirty minutes" -
# was a measurement of a busy machine (rounds 166-168). Tracking each child's PID was half the fix; the other half needs
# the operating system, because Windows PowerShell does not run a `finally` block when it is killed.
#
# A job object is that half. `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE` terminates every process in the job when its last
# handle closes, and the handle closes when the process holding it dies - killed or not. Descendants join the job by
# inheritance, so assigning this one process protects every check it will start.
#
# Windows 8 and later allow nested jobs, so this works even when the runner was started inside somebody else's job.
# If it cannot be installed the function says so and returns false: the caller may keep the sweep it already has, but it
# must not claim children are being taken down.

Add-Type -TypeDefinition @'
using System;
using System.Diagnostics;
using System.Runtime.InteropServices;

public static class UiLmJob
{
    private const int JobObjectExtendedLimitInformation = 9;
    private const uint JobObjectLimitKillOnJobClose = 0x2000;

    [DllImport("kernel32.dll", CharSet = CharSet.Unicode, SetLastError = true)]
    private static extern IntPtr CreateJobObject(IntPtr attributes, string name);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool SetInformationJobObject(IntPtr job, int infoClass, IntPtr info, uint length);

    [DllImport("kernel32.dll", SetLastError = true)]
    private static extern bool AssignProcessToJobObject(IntPtr job, IntPtr process);

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_BASIC_LIMIT_INFORMATION
    {
        public long PerProcessUserTimeLimit;
        public long PerJobUserTimeLimit;
        public uint LimitFlags;
        public UIntPtr MinimumWorkingSetSize;
        public UIntPtr MaximumWorkingSetSize;
        public uint ActiveProcessLimit;
        public UIntPtr Affinity;
        public uint PriorityClass;
        public uint SchedulingClass;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct IO_COUNTERS
    {
        public ulong ReadOperationCount;
        public ulong WriteOperationCount;
        public ulong OtherOperationCount;
        public ulong ReadTransferCount;
        public ulong WriteTransferCount;
        public ulong OtherTransferCount;
    }

    [StructLayout(LayoutKind.Sequential)]
    private struct JOBOBJECT_EXTENDED_LIMIT_INFORMATION
    {
        public JOBOBJECT_BASIC_LIMIT_INFORMATION BasicLimitInformation;
        public IO_COUNTERS IoInfo;
        public UIntPtr ProcessMemoryLimit;
        public UIntPtr JobMemoryLimit;
        public UIntPtr PeakProcessMemoryUsed;
        public UIntPtr PeakJobMemoryUsed;
    }

    // The process's only handle to the job. Closing it would kill the members, so it is kept for the process's life and
    // never closed: the operating system closes it when this process ends, which is exactly when the kill must happen.
    private static IntPtr job_ = IntPtr.Zero;

    public static bool AttachCurrentProcess(out string error)
    {
        error = null;
        if (job_ != IntPtr.Zero) { return true; }

        IntPtr job = CreateJobObject(IntPtr.Zero, null);
        if (job == IntPtr.Zero)
        {
            error = "CreateJobObject failed with " + Marshal.GetLastWin32Error();
            return false;
        }

        JOBOBJECT_EXTENDED_LIMIT_INFORMATION info = new JOBOBJECT_EXTENDED_LIMIT_INFORMATION();
        info.BasicLimitInformation.LimitFlags = JobObjectLimitKillOnJobClose;
        int size = Marshal.SizeOf(typeof(JOBOBJECT_EXTENDED_LIMIT_INFORMATION));
        IntPtr buffer = Marshal.AllocHGlobal(size);
        try
        {
            Marshal.StructureToPtr(info, buffer, false);
            if (!SetInformationJobObject(job, JobObjectExtendedLimitInformation, buffer, (uint)size))
            {
                error = "SetInformationJobObject failed with " + Marshal.GetLastWin32Error();
                return false;
            }
        }
        finally
        {
            Marshal.FreeHGlobal(buffer);
        }

        if (!AssignProcessToJobObject(job, Process.GetCurrentProcess().Handle))
        {
            error = "AssignProcessToJobObject failed with " + Marshal.GetLastWin32Error();
            return false;
        }

        job_ = job;
        return true;
    }
}
'@

function Enter-KillOnCloseJob {
    $detail = $null
    if ([UiLmJob]::AttachCurrentProcess([ref]$detail)) { return $true }
    Write-Host ("  the job object could not be installed: " + $detail)
    return $false
}
