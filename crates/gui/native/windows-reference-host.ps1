# Task-owned stock Microsoft multiline EDIT reference; no application endpoint.
$ErrorActionPreference = 'Stop'
Add-Type -AssemblyName System.Windows.Forms
$form = [Windows.Forms.Form]::new()
$form.Text = 'Native UIA stock reference'
$form.Width = 700
$form.Height = 400
$form.StartPosition = [Windows.Forms.FormStartPosition]::CenterScreen
$edit = [Windows.Forms.TextBox]::new()
$edit.Multiline = $true
$edit.AcceptsReturn = $true
$edit.AccessibleName = 'Custom recipe notes'
$edit.Dock = [Windows.Forms.DockStyle]::Fill
$form.Controls.Add($edit)
$form.Add_Shown({ $edit.Focus() | Out-Null })
try { $form.ShowDialog() | Out-Null } finally { $form.Dispose() }
