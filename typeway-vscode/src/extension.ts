import * as vscode from 'vscode';
import * as child_process from 'child_process';
import * as path from 'path';

export function activate(context: vscode.ExtensionContext) {
    // Convert Axum -> Typeway (modifies file)
    context.subscriptions.push(
        vscode.commands.registerCommand('typeway.convertToTypeway', async () => {
            await runConversion('axum-to-typeway', false);
        })
    );

    // Convert Typeway -> Axum (modifies file)
    context.subscriptions.push(
        vscode.commands.registerCommand('typeway.convertToAxum', async () => {
            await runConversion('typeway-to-axum', false);
        })
    );

    // Preview Axum -> Typeway (dry run, shows in new tab)
    context.subscriptions.push(
        vscode.commands.registerCommand('typeway.convertToTypewayDryRun', async () => {
            await runConversion('axum-to-typeway', true);
        })
    );

    // Preview Typeway -> Axum (dry run, shows in new tab)
    context.subscriptions.push(
        vscode.commands.registerCommand('typeway.convertToAxumDryRun', async () => {
            await runConversion('typeway-to-axum', true);
        })
    );

    // Check current file
    context.subscriptions.push(
        vscode.commands.registerCommand('typeway.checkFile', async () => {
            await runCheck();
        })
    );
}

async function runConversion(direction: string, dryRun: boolean) {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
    }

    const filePath = editor.document.uri.fsPath;
    if (!filePath.endsWith('.rs')) {
        vscode.window.showErrorMessage('Not a Rust file');
        return;
    }

    // Save the file first
    await editor.document.save();

    const config = vscode.workspace.getConfiguration('typeway');
    const binaryPath = config.get<string>('migrateBinaryPath', 'typeway-migrate');

    const args = [direction, '--file', filePath];
    if (dryRun) {
        args.push('--dry-run');
    }

    try {
        const result = await execCommand(binaryPath, args);

        if (dryRun) {
            // Show the converted output in a new untitled editor
            const doc = await vscode.workspace.openTextDocument({
                language: 'rust',
                content: result.stdout,
            });
            await vscode.window.showTextDocument(doc, vscode.ViewColumn.Beside);
            vscode.window.showInformationMessage(`Preview: ${direction} conversion`);
        } else {
            // Reload the file (it was modified in place)
            await vscode.commands.executeCommand('workbench.action.revertFile');
            vscode.window.showInformationMessage(
                `Converted ${path.basename(filePath)} (backup: ${path.basename(filePath)}.bak)`
            );
        }

        // Show any stderr output (warnings, summary) in the output channel
        if (result.stderr) {
            const channel = getOutputChannel();
            channel.appendLine(result.stderr);
            channel.show(true);
        }
    } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        vscode.window.showErrorMessage(`Conversion failed: ${message}`);
    }
}

async function runCheck() {
    const editor = vscode.window.activeTextEditor;
    if (!editor) {
        vscode.window.showErrorMessage('No active editor');
        return;
    }

    const filePath = editor.document.uri.fsPath;
    if (!filePath.endsWith('.rs')) {
        vscode.window.showErrorMessage('Not a Rust file');
        return;
    }

    await editor.document.save();

    const config = vscode.workspace.getConfiguration('typeway');
    const binaryPath = config.get<string>('migrateBinaryPath', 'typeway-migrate');

    try {
        const result = await execCommand(binaryPath, ['check', '--file', filePath]);
        const channel = getOutputChannel();
        channel.clear();
        channel.appendLine(result.stdout);
        if (result.stderr) {
            channel.appendLine(result.stderr);
        }
        channel.show(true);
    } catch (error: unknown) {
        const message = error instanceof Error ? error.message : String(error);
        vscode.window.showErrorMessage(`Check failed: ${message}`);
    }
}

function execCommand(command: string, args: string[]): Promise<{ stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
        child_process.execFile(command, args, { maxBuffer: 10 * 1024 * 1024 }, (error, stdout, stderr) => {
            if (error && error.code !== undefined) {
                reject(new Error(`${command} failed: ${stderr || error.message}`));
            } else {
                resolve({ stdout: stdout || '', stderr: stderr || '' });
            }
        });
    });
}

let outputChannel: vscode.OutputChannel | undefined;
function getOutputChannel(): vscode.OutputChannel {
    if (!outputChannel) {
        outputChannel = vscode.window.createOutputChannel('Typeway');
    }
    return outputChannel;
}

export function deactivate() {}
