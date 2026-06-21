#!/usr/bin/env node

const { program } = require('commander');
const sharp = require('sharp');
const fs = require('fs');
const path = require('path');
const { globSync } = require('glob');
const chokidar = require('chokidar');

program
  .name('vexell')
  .description('Vexell: Blazing fast lossless SVG to image converter')
  .version('1.1.0')
  .argument('[input]', 'Input SVG file or glob pattern (leave empty for interactive mode)')
  .argument('[output]', 'Output file or directory (required if not using watch with same dir)')
  .option('-s, --size <pixels>', 'Output resolution (width and height)', 1024)
  .option('-f, --format <type>', 'Output format (png, webp, avif, jpeg)', 'png')
  .option('-b, --background <color>', 'Background color (e.g., "#ffffff", "white")')
  .option('-O, --optimize', 'Optimize output (higher compression)', false)
  .option('-w, --watch', 'Watch input files for changes', false)
  .action(async (inputPattern, output, options) => {
    // Import ESM packages dynamically
    const chalk = (await import('chalk')).default;
    const ora = (await import('ora')).default;
    const figlet = require('figlet');

    if (!inputPattern) {
      try {
        const { input, select, confirm } = await import('@inquirer/prompts');
        
        console.clear();
        console.log(chalk.cyanBright(figlet.textSync('Vexell', { font: 'Slant' })));
        console.log(chalk.gray('Blazing fast lossless SVG to image converter\n'));

        inputPattern = await input({ 
          message: chalk.white('Enter input SVG file or glob pattern (e.g., icon.svg, src/*.svg):'),
          validate: (val) => val.trim().length > 0 || 'Input pattern is required'
        });
        
        options.watch = await confirm({ message: chalk.white('Enable watch mode?'), default: false });

        options.format = await select({
          message: chalk.white('Select output format:'),
          choices: [
            { name: 'PNG', value: 'png' },
            { name: 'WebP', value: 'webp' },
            { name: 'AVIF', value: 'avif' },
            { name: 'JPEG', value: 'jpeg' },
          ],
        });

        options.size = await input({ 
          message: chalk.white('Enter output resolution in pixels:'), 
          default: '1024' 
        });

        const addBg = await confirm({ message: chalk.white('Add a background color?'), default: false });
        if (addBg) {
          options.background = await input({ message: chalk.white('Enter background color (e.g. #ffffff or white):'), default: '#ffffff' });
        }

        options.optimize = await confirm({ message: chalk.white('Enable aggressive optimization?'), default: false });

        const outInput = await input({ message: chalk.white('Enter output file or directory (leave blank to auto-generate):') });
        output = outInput.trim() || undefined;
        
      } catch (err) {
        if (err.name === 'ExitPromptError') {
          console.log(chalk.yellow('\nAborted.'));
          process.exit(0);
        }
        console.error(chalk.red('Error in interactive prompt:'), err);
        process.exit(1);
      }
    }

    const size = parseInt(options.size);
    const format = options.format.toLowerCase();
    
    if (!['png', 'webp', 'avif', 'jpeg', 'jpg'].includes(format)) {
      const chalk = (await import('chalk')).default;
      console.error(chalk.red(`Error: Unsupported format ${format}`));
      process.exit(1);
    }

    const isOutputDirectory = !output || output.endsWith('/') || output.endsWith('\\') || (fs.existsSync(output) && fs.statSync(output).isDirectory()) || !output.includes('.');

    const processFile = async (filePath) => {
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

      let pipeline = sharp(filePath).resize(size, size, { fit: 'inside' });

      if (options.background) {
        pipeline = pipeline.flatten({ background: options.background });
      }

      if (format === 'png') {
        pipeline = pipeline.png({ 
          compressionLevel: options.optimize ? 9 : 6, 
          adaptiveFiltering: true, 
          force: true 
        });
      } else if (format === 'webp') {
        pipeline = pipeline.webp({ quality: options.optimize ? 80 : 100, lossless: !options.optimize });
      } else if (format === 'avif') {
        pipeline = pipeline.avif({ quality: options.optimize ? 50 : 80, lossless: !options.optimize });
      } else if (format === 'jpeg' || format === 'jpg') {
        pipeline = pipeline.jpeg({ quality: options.optimize ? 80 : 100 });
      }

      try {
        const info = await pipeline.toFile(outPath);
        spinner.succeed(`Rendered ${chalk.greenBright(outPath)} ${chalk.gray(`(${info.width}x${info.height})`)}`);
      } catch (err) {
        spinner.fail(chalk.red(`Failed ${filePath}: ${err.message}`));
      }
    };

    if (options.watch) {
      console.log(chalk.blueBright(`\n👀 Watching for changes: ${inputPattern}\n`));
      const watcher = chokidar.watch(inputPattern, { persistent: true });
      watcher
        .on('add', processFile)
        .on('change', processFile)
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
      await Promise.all(files.map(processFile));
      console.log(chalk.greenBright(`\n✨ All tasks completed successfully!\n`));
    }
  });

program.parse();
