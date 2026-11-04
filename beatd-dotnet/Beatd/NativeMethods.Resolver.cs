#if NET5_0_OR_GREATER
using System;
using System.Reflection;
using System.Runtime.InteropServices;

namespace Beatd
{
    partial class NativeMethods
    {
        static NativeMethods()
        {
            NativeLibrary.SetDllImportResolver(typeof(NativeMethods).Assembly, Resolve);
        }

        static IntPtr Resolve(string name, Assembly assembly, DllImportSearchPath? searchPath)
        {
            if (
                name == __DllName
                && NativeLibrary.TryLoad(name, assembly, searchPath, out var handle)
            )
                return handle;
            return IntPtr.Zero;
        }
    }
}
#endif
