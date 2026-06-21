const pty = require('node-pty');

const vexell = pty.spawn('node', ['index.js'], {
  name: 'xterm-color',
  cols: 80,
  rows: 30,
  cwd: process.cwd(),
  env: process.env
});

let output = '';

vexell.onData((data) => {
  output += data;
  if (data.includes('Enter input SVG file')) {
    console.log('Sending ESC key...');
    vexell.write('\x1b');
  }
});

vexell.onExit(({ exitCode }) => {
  console.log('Exited with code:', exitCode);
  console.log('Output:\n', output);
});
