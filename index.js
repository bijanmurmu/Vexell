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
    
    if (!inputPattern) {
      try {
        const { input, select, confirm } = await import('@inquirer/prompts');
        
        console.log('\n✨ Welcome to Vexell Interactive Mode ✨\n');

        inputPattern = await input({ 
          message: 'Enter input SVG file or glob pattern (e.g., icon.svg, src/*.svg):',
          validate: (val) => val.trim().length > 0 || 'Input pattern is required'
        });
        
        options.watch = await confirm({ message: 'Enable watch mode?', default: false });

        options.format = await select({
          message: 'Select output format:',
          choices: [
            { name: 'PNG', value: 'png' },
            { name: 'WebP', value: 'webp' },
            { name: 'AVIF', value: 'avif' },
            { name: 'JPEG', value: 'jpeg' },
          ],
        });

        options.size = await input({ 
          message: 'Enter output resolution in pixels:', 
          default: '1024' 
        });

        const addBg = await confirm({ message: 'Add a background color?', default: false });
        if (addBg) {
          options.background = await input({ message: 'Enter background color (e.g. #ffffff or white):', default: '#ffffff' });
        }

        options.optimize = await confirm({ message: 'Enable aggressive optimization?', default: false });

        const outInput = await input({ message: 'Enter output file or directory (leave blank to auto-generate):' });
        output = outInput.trim() || undefined;
        
        console.log('\n🚀 Starting conversion...\n');
      } catch (err) {
        if (err.name === 'ExitPromptError') {
          console.log('\nAborted.');
          process.exit(0);
        }
        console.error('Error in interactive prompt:', err);
        process.exit(1);
      }
    }

    const size = parseInt(options.size);
    const format = options.format.toLowerCase();
    
    if (!['png', 'webp', 'avif', 'jpeg', 'jpg'].includes(format)) {
      console.error(`Error: Unsupported format ${format}`);
      process.exit(1);
    }

    const isOutputDirectory = !output || output.endsWith('/') || output.endsWith('\\') || (fs.existsSync(output) && fs.statSync(output).isDirectory()) || !output.includes('.');

    const processFile = async (filePath) => {
      let outPath = output;
      
      if (!output) {
         // Default to same directory, same name, new extension
         outPath = filePath.replace(/\.svg$/i, `.${format}`);
      } else if (isOutputDirectory) {
         if (!fs.existsSync(output)) {
           fs.mkdirSync(output, { recursive: true });
         }
         const baseName = path.basename(filePath, path.extname(filePath));
         outPath = path.join(output, `${baseName}.${format}`);
      }

      console.log(`Vexellizing ${filePath} -> ${outPath}...`);

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
        console.log(`Successfully rendered! (${info.width}x${info.height})`);
      } catch (err) {
        console.error(`Error converting file ${filePath}: ${err.message}`);
      }
    };

    if (options.watch) {
      console.log(`Watching for changes: ${inputPattern}`);
      const watcher = chokidar.watch(inputPattern, { persistent: true });
      watcher
        .on('add', processFile)
        .on('change', processFile)
        .on('error', error => console.error(`Watcher error: ${error}`));
    } else {
      // Fix glob on windows (needs forward slashes)
      const normalizedPattern = inputPattern.replace(/\\/g, '/');
      const files = globSync(normalizedPattern);
      
      if (files.length === 0) {
        console.error(`Error: No files found matching ${inputPattern}`);
        process.exit(1);
      }
      
      if (files.length > 1 && !isOutputDirectory && output) {
        console.error(`Error: Output must be a directory when processing multiple files.`);
        process.exit(1);
      }

      await Promise.all(files.map(processFile));
    }
  });

program.parse();
