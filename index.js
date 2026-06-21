#!/usr/bin/env node

const { program } = require('commander');
const sharp = require('sharp');
const fs = require('fs');
const path = require('path');
const { globSync } = require('glob');
const chokidar = require('chokidar');
const readline = require('readline');

program
  .name('vexell')
  .description('Vexell: Blazing fast lossless SVG to image converter')
  .version('1.1.0')
  .argument('[input]', 'Input SVG file or glob pattern (leave empty for interactive mode)')
  .option('-b, --background <color>', 'Background color (e.g., "#ffffff", "white")')
  .option('-O, --optimize', 'Optimize output (higher compression)', false)
  .option('-w, --watch', 'Watch input files for changes', false)
  .action(async (inputPattern, output, options) => {
    const chalk = (await import('chalk')).default;
    const ora = (await import('ora')).default;
    const figlet = require('figlet');

    readline.emitKeypressEvents(process.stdin);
    if (process.stdin.isTTY) {
      process.stdin.setRawMode(true);
      process.stdin.resume();
    }

    if (!inputPattern) {
      const { input, select, confirm } = await import('@inquirer/prompts');
      
      let isWatching = false;
      let currentWatcher = null;
      
      // Map ESC to Ctrl+C so Inquirer natively aborts the prompt
      const globalEscListener = (str, key) => {
        if (key && key.name === 'escape') {
          if (isWatching && currentWatcher) {
             currentWatcher.close();
             isWatching = false;
             currentWatcher = null;
          } else {
             // Simulate Ctrl+C to tell Inquirer to abort
             process.stdin.emit('keypress', '\x03', { name: 'c', ctrl: true, meta: false, shift: false });
          }
        }
      };
      process.stdin.on('keypress', globalEscListener);

      const mainMenuLoop = async () => {
        while (true) {
          console.clear();
          console.log(chalk.cyanBright(figlet.textSync('Vexell', { font: 'Slant' })));
          console.log(chalk.gray('Interactive Console Mode. Press ESC anytime to cancel operation.\n'));

          try {
            const action = await select({
              message: chalk.white('Main Menu:'),
              choices: [
                { name: '🪄 Start Conversion Wizard', value: 'wizard' },
                { name: '👀 Start Watch Mode', value: 'watch' },
                { name: '🚪 Exit Vexell', value: 'exit' }
              ]
            });

            if (action === 'exit') {
              console.log(chalk.green('\nGoodbye! ✨\n'));
              process.exit(0);
            }

            console.log();
            let pat = await input({ 
              message: chalk.white('Enter input SVG file or glob pattern:'),
              validate: (val) => val.trim().length > 0 || 'Input is required'
            });

            let format = await select({
              message: chalk.white('Select output format:'),
              choices: [
                { name: 'PNG', value: 'png' },
                { name: 'WebP', value: 'webp' },
                { name: 'AVIF', value: 'avif' },
                { name: 'JPEG', value: 'jpeg' },
              ]
            });

            let size = await input({ message: chalk.white('Enter output resolution in pixels:'), default: '1024' });

            let bg = null;
            const addBg = await confirm({ message: chalk.white('Add a background color?'), default: false });
            if (addBg) {
              bg = await input({ message: chalk.white('Enter background color (e.g. #ffffff):'), default: '#ffffff' });
            }

            let opt = await confirm({ message: chalk.white('Enable aggressive optimization?'), default: false });

            let outInput = await input({ message: chalk.white('Enter output file or directory (leave blank to auto-generate):') });
            let out = outInput.trim() || undefined;

            if (action === 'watch') {
               console.log(chalk.yellow('\n[Press ESC at any time to stop watching and return to the main menu]\n'));
               
               // Inquirer pauses stdin after prompts. We MUST resume it to catch ESC.
               if (process.stdin.isTTY) {
                 process.stdin.setRawMode(true);
                 process.stdin.resume();
               }

               isWatching = true;
               
               await new Promise((resolve) => {
                 currentWatcher = chokidar.watch(pat, { persistent: true });
                 currentWatcher
                   .on('add', (fp) => doProcess(fp, out, size, format, bg, opt, chalk, ora))
                   .on('change', (fp) => doProcess(fp, out, size, format, bg, opt, chalk, ora))
                   .on('error', err => console.error(chalk.red(`Watcher error: ${err}`)));
                 
                 const checkInterval = setInterval(() => {
                   if (!isWatching) {
                     clearInterval(checkInterval);
                     console.log(chalk.yellow('\nWatch mode stopped. Returning to menu...\n'));
                     setTimeout(resolve, 1000);
                   }
                 }, 200);
               });
            } else {
               const normalizedPattern = pat.replace(/\\/g, '/');
               const files = globSync(normalizedPattern);
               if (files.length === 0) {
                 console.log(chalk.red(`\nNo files found matching ${pat}\n`));
                 await new Promise(r => setTimeout(r, 2000));
                 continue;
               }
               console.log(chalk.blueBright(`\n🚀 Starting conversion for ${files.length} file(s)...\n`));
               await Promise.all(files.map(f => doProcess(f, out, size, format, bg, opt, chalk, ora)));
               console.log(chalk.greenBright(`\n✨ Done! Returning to menu...\n`));
               await new Promise(r => setTimeout(r, 2000));
            }

          } catch (err) {
            if (err.name === 'ExitPromptError') {
              console.log(chalk.yellow('\nOperation cancelled. Returning to menu...\n'));
              await new Promise(r => setTimeout(r, 1000));
            } else {
              console.error(chalk.red('\nError:'), err);
              process.exit(1);
            }
          }
        }
      };

      await mainMenuLoop();
      return; 
    }

    // ---- Standard CLI Mode (arguments provided) ----
    process.stdin.on('keypress', (str, key) => {
      if (key && (key.name === 'escape' || (key.ctrl && key.name === 'c'))) {
        console.log('\n\x1b[33mAborted.\x1b[0m'); 
        process.exit(0);
      }
    });

    const size = parseInt(options.size);
    const format = options.format.toLowerCase();
    
    if (!['png', 'webp', 'avif', 'jpeg', 'jpg'].includes(format)) {
      console.error(chalk.red(`Error: Unsupported format ${format}`));
      process.exit(1);
    }

    const isOutputDirectory = !output || output.endsWith('/') || output.endsWith('\\') || (fs.existsSync(output) && fs.statSync(output).isDirectory()) || !output.includes('.');

    if (options.watch) {
      console.log(chalk.blueBright(`\n👀 Watching for changes: ${inputPattern}\n`));
      console.log(chalk.yellow(`[Press ESC to abort]\n`));
      const watcher = chokidar.watch(inputPattern, { persistent: true });
      watcher
        .on('add', (fp) => doProcess(fp, output, size, format, options.background, options.optimize, chalk, ora, isOutputDirectory))
        .on('change', (fp) => doProcess(fp, output, size, format, options.background, options.optimize, chalk, ora, isOutputDirectory))
        .on('error', error => console.error(chalk.red(`Watcher error: ${error}`)));
    } else {
      const normalizedPattern = inputPattern.replace(/\\/g, '/');
      const files = globSync(normalizedPattern);
      
      if (files.length === 0) {
        console.error(chalk.red(`\nError: No files found matching ${inputPattern}\n`));
        process.exit(1);
      }
      
      if (files.length > 1 && !isOutputDirectory && output) {
        console.error(chalk.red(`\nError: Output must be a directory when processing multiple files.\n`));
        process.exit(1);
      }

      console.log(chalk.blueBright(`\n🚀 Starting conversion for ${files.length} file(s)...\n`));
      await Promise.all(files.map(fp => doProcess(fp, output, size, format, options.background, options.optimize, chalk, ora, isOutputDirectory)));
      console.log(chalk.greenBright(`\n✨ All tasks completed successfully!\n`));
      process.exit(0);
    }
  });

async function doProcess(filePath, output, size, format, background, optimize, chalk, ora, isOutputDirectoryParam) {
    const isOutputDirectory = isOutputDirectoryParam ?? (!output || output.endsWith('/') || output.endsWith('\\') || (fs.existsSync(output) && fs.statSync(output).isDirectory()) || !output.includes('.'));
    
    let outPath = output;
      
    if (!output) {
        outPath = filePath.replace(/\.svg$/i, `.${format}`);
    } else if (isOutputDirectory) {
        if (!fs.existsSync(output)) {
          fs.mkdirSync(output, { recursive: true });
        }
        const baseName = path.basename(filePath, path.extname(filePath));
        outPath = path.join(output, `${baseName}.${format}`);
    }

    const spinner = ora(`Vexellizing ${chalk.cyan(filePath)} -> ${chalk.greenBright(outPath)}`).start();

    let pipeline = sharp(filePath).resize(parseInt(size), parseInt(size), { fit: 'inside' });

    if (background) {
      pipeline = pipeline.flatten({ background: background });
    }

    if (format === 'png') {
      pipeline = pipeline.png({ 
        compressionLevel: optimize ? 9 : 6, 
        adaptiveFiltering: true, 
        force: true 
      });
    } else if (format === 'webp') {
      pipeline = pipeline.webp({ quality: optimize ? 80 : 100, lossless: !optimize });
    } else if (format === 'avif') {
      pipeline = pipeline.avif({ quality: optimize ? 50 : 80, lossless: !optimize });
    } else if (format === 'jpeg' || format === 'jpg') {
      pipeline = pipeline.jpeg({ quality: optimize ? 80 : 100 });
    }

    try {
      const info = await pipeline.toFile(outPath);
      spinner.succeed(`Rendered ${chalk.greenBright(outPath)} ${chalk.gray(`(${info.width}x${info.height})`)}`);
    } catch (err) {
      spinner.fail(chalk.red(`Failed ${filePath}: ${err.message}`));
    }
}

program.parse();
