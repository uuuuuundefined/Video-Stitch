// 只诊断本次 CI 应用的本机调试端口，不读取浏览器账号或其他进程。
try {
  const pages = await (await fetch("http://127.0.0.1:9222/json/list")).json();
  const page = pages.find((item) => item.type === "page");
  if (!page) throw new Error("No application page");
  const socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener("open", resolve, { once: true });
    socket.addEventListener("error", reject, { once: true });
  });
  const expression = process.argv[2] === "exit"
    ? "(async()=>{try{await window.__TAURI_INTERNALS__.invoke('exit_app');return 'exit returned'}catch(error){return JSON.stringify(error)}})()"
    : "JSON.stringify({title:document.title,ready:document.readyState,text:document.body.innerText})";
  const result = await new Promise((resolve, reject) => {
    const timeout = setTimeout(() => reject(new Error("Application evaluation timed out")), 10000);
    socket.addEventListener("message", ({ data }) => {
      const message = JSON.parse(data);
      if (message.id === 1) { clearTimeout(timeout); resolve(message); }
    });
    socket.addEventListener("close", () => { clearTimeout(timeout); resolve({ closed: true }); }, { once: true });
    socket.send(JSON.stringify({ id: 1, method: "Runtime.evaluate", params: { expression, awaitPromise: true, returnByValue: true } }));
  });
  console.log("APPLICATION", process.argv[2], JSON.stringify(result));
  socket.close();
} catch (error) {
  console.log("APPLICATION DIAGNOSTIC", error.message);
}
