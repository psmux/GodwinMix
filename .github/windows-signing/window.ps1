# Filling in SimplySign Desktop's login window. Dot sourced by login.ps1.
#
# The first attempt typed into the window with SendKeys alone and the token
# field came out a digit short. So the two text boxes are now found as child
# windows and given their text with WM_SETTEXT, which needs no focus and
# loses no keystrokes, and works on a password box too. If the window has no
# such child windows (a toolkit that draws its own controls), the fallback
# types one character at a time.
Add-Type -Namespace GmxSign -Name Win32 -MemberDefinition @'
[DllImport("user32.dll")] public static extern bool EnumChildWindows(IntPtr parent, EnumProc cb, IntPtr l);
public delegate bool EnumProc(IntPtr h, IntPtr l);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern int GetClassName(IntPtr h, System.Text.StringBuilder s, int n);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, string l);
[DllImport("user32.dll", CharSet = CharSet.Unicode)] public static extern IntPtr SendMessage(IntPtr h, uint m, IntPtr w, System.Text.StringBuilder l);
[DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
[DllImport("user32.dll")] public static extern bool IsWindowVisible(IntPtr h);
[DllImport("user32.dll")] public static extern bool PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
[StructLayout(LayoutKind.Sequential)] public struct RECT { public int Left, Top, Right, Bottom; }
[DllImport("user32.dll")] public static extern bool EnumWindows(EnumProc cb, IntPtr l);
[DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, out uint pid);
public static System.Collections.Generic.List<IntPtr> TopLevel(int pid) {
    var list = new System.Collections.Generic.List<IntPtr>();
    EnumWindows((h, l) => { uint p; GetWindowThreadProcessId(h, out p); if (p == pid && IsWindowVisible(h)) list.Add(h); return true; }, IntPtr.Zero);
    return list;
}
public static System.Collections.Generic.List<IntPtr> Children(IntPtr parent) {
    var list = new System.Collections.Generic.List<IntPtr>();
    EnumChildWindows(parent, (h, l) => { list.Add(h); return true; }, IntPtr.Zero);
    return list;
}
'@

function Get-ChildControls($Window, [IntPtr] $Parent = $Window.MainWindowHandle) {
    foreach ($h in [GmxSign.Win32]::Children($Parent)) {
        $sb = [System.Text.StringBuilder]::new(256)
        [void][GmxSign.Win32]::GetClassName($h, $sb, 256)
        $r = New-Object GmxSign.Win32+RECT
        [void][GmxSign.Win32]::GetWindowRect($h, [ref]$r)
        [pscustomobject]@{ Handle = $h; Class = $sb.ToString(); Top = $r.Top; Left = $r.Left
                           Visible = [GmxSign.Win32]::IsWindowVisible($h) }
    }
}

# For the log: the window's controls by class and position, never their text.
function Write-UiTree($Window) {
    Get-ChildControls $Window | ForEach-Object { Write-Host "  $($_.Class) top=$($_.Top) left=$($_.Left) visible=$($_.Visible)" }
}

# The two text boxes, the account above the token, or $null.
function Get-LoginFields($Window) {
    $edits = @(Get-ChildControls $Window | Where-Object { $_.Visible -and $_.Class -match 'edit' } | Sort-Object Top, Left)
    if ($edits.Count -lt 2) { return $null }
    return , @($edits[0].Handle, $edits[1].Handle)
}

# WM_SETTEXT, then WM_GETTEXTLENGTH to see the field took it. Windows still
# reports the length of a password box's text to another process.
function Set-Field([IntPtr] $Handle, [string] $Text) {
    [void][GmxSign.Win32]::SendMessage($Handle, 0x000C, [IntPtr]::Zero, $Text)
    $len = [GmxSign.Win32]::SendMessage($Handle, 0x000E, [IntPtr]::Zero, [string]$null).ToInt32()
    return $len -eq $Text.Length
}

function Get-ControlText([IntPtr] $Handle) {
    $sb = [System.Text.StringBuilder]::new(512)
    [void][GmxSign.Win32]::SendMessage($Handle, 0x000D, [IntPtr]512, $sb)
    return $sb.ToString()
}

# Presses Ok. Enter alone was lost once when the window did not have the
# keyboard, so the button is clicked with BM_CLICK, posted rather than sent
# because the click may open a modal message box. Enter is the fallback.
function Submit-Login($Window) {
    Send-ToWindow $Window ''
    $ok = Get-ChildControls $Window | Where-Object { $_.Class -match 'button' -and (Get-ControlText $_.Handle) -match '^&?ok$' } |
        Select-Object -First 1
    if ($ok) { [void][GmxSign.Win32]::PostMessage($ok.Handle, 0x00F5, [IntPtr]::Zero, [IntPtr]::Zero); return }
    Write-Host 'no Ok button found; pressing Enter'
    Send-ToWindow $Window '{ENTER}'
}

# Empties the account field, so a screenshot of a failed login shows no name.
function Clear-AccountField($Window) {
    $fields = Get-LoginFields $Window
    if ($fields) { [void](Set-Field $fields[0] '') }
}

# The fallback: the account field has focus when the window opens.
function Send-Slowly($Window, [string] $Text) {
    foreach ($ch in $Text.ToCharArray()) {
        Send-ToWindow $Window (ConvertTo-SendKeysText ([string]$ch))
        Start-Sleep -Milliseconds 80
    }
}

function Set-LoginFields($Window, [string] $User, [string] $Code) {
    $fields = Get-LoginFields $Window
    if ($fields) {
        $a = Set-Field $fields[0] $User
        $b = Set-Field $fields[1] $Code
        Write-Host "fields set by WM_SETTEXT: account $a, token $b"
        return
    }
    Write-Host 'no text box child windows; typing instead'
    Send-ToWindow $Window '^a'; Start-Sleep -Milliseconds 300
    Send-Slowly $Window $User
    Send-ToWindow $Window '{TAB}'; Start-Sleep -Milliseconds 300
    Send-ToWindow $Window '^a'; Start-Sleep -Milliseconds 300
    Send-Slowly $Window $Code
}

# The text of any message box SimplySign has open, such as its "Invalid user
# name or token", so the log says why a login failed.
function Get-SimplySignMessages {
    $tops = Get-Process | Where-Object { $_.ProcessName -like '*SimplySign*' } |
        ForEach-Object { [GmxSign.Win32]::TopLevel($_.Id) }
    foreach ($top in $tops) {
        foreach ($c in Get-ChildControls $null $top) {
            if ($c.Class -notmatch 'static') { continue }
            $text = Get-ControlText $c.Handle
            if ($text) { $text }
        }
    }
}
