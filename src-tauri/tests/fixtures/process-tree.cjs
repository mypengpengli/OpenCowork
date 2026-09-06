const { spawn } = require('node:child_process')
const child = spawn(process.execPath, ['-e', 'setInterval(() => {}, 1000)'], { stdio: 'inherit', windowsHide: true })
console.log(`child_pid: ${child.pid}`)
setInterval(() => {}, 1000)
