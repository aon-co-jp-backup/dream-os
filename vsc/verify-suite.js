// Runs inside the built DreamOS Code (passed as --extensionTestsPath): the bundled extension must activate,
// register its commands and open its webview.  Prints a marker line that verify.sh greps for.
const vscode = require('vscode');

exports.run = async function run() {
  const ext = vscode.extensions.getExtension('aon-co-jp.open-vsc-claude');
  if (!ext) throw new Error('open-vsc-claude is not installed in this build');
  await ext.activate();
  const cmds = await vscode.commands.getCommands(true);
  for (const c of ['openVscClaude.open', 'openVscClaude.setApiKey', 'openVscClaude.loadApiKeyFromFile', 'openVscClaude.checkForUpdates']) {
    if (!cmds.includes(c)) throw new Error('command missing: ' + c);
  }
  await vscode.commands.executeCommand('openVscClaude.open');
  const findTab = () => vscode.window.tabGroups.all.flatMap((g) => g.tabs).find((t) => t.input instanceof vscode.TabInputWebview);
  for (let i = 0; i < 60 && !findTab(); i++) await new Promise((r) => setTimeout(r, 250));
  if (!findTab()) throw new Error('the picker webview did not open');
  console.log('DREAMOS-VERIFY-OK app=' + vscode.env.appName + ' version=' + vscode.version + ' ext=' + ext.packageJSON.version);
};
