import { useState } from 'react'
import './App.css'

function App() {
  const [currentPage, setCurrentPage] = useState<'home' | 'game'>('home')

  if (currentPage === 'game') {
    return (
      <main className="app-shell app-shell--game">
        <button
          className="back-button"
          type="button"
          onClick={() => setCurrentPage('home')}
        >
          返回首页
        </button>
        <iframe
          className="game-frame"
          src="/games/typing/index.html"
          title="打字游戏"
          allow="autoplay"
        />
      </main>
    )
  }

  return (
    <main className="app-shell">
      <section className="hero-card">
        <p className="eyebrow">aisuixue.com</p>
        <h1>AI 随学</h1>
        <p className="description">原来的贪食蛇已经移除，这里改成新的打字闯关游戏入口。</p>
        <button
          className="start-button"
          type="button"
          onClick={() => setCurrentPage('game')}
        >
          进入游戏
        </button>
      </section>
    </main>
  )
}

export default App
