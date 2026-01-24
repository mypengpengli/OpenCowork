import { useState } from 'react'
import './App.css'
import Game from './Game'

function App() {
  const [currentPage, setCurrentPage] = useState<'home' | 'game'>('home')

  if (currentPage === 'game') {
    return <Game onBack={() => setCurrentPage('home')} />
  }

  return (
    <div className="container">
      <h1>Hello World!</h1>
      <p>欢迎来到 AI随学 - aisuixue.com</p>
      <p>网站部署成功�?/p>
      <div style={{ margin: '2rem 0' }}>
        <a 
          href="#" 
          onClick={(e) => {
            e.preventDefault()
            setCurrentPage('game')
          }}
          style={{
            color: '#fff',
            textDecoration: 'underline',
            fontSize: '1.5rem',
            fontWeight: 'bold'
          }}
        >
          进入游戏
        </a>
      </div>
      <p className="tech-stack">Vite + React + TypeScript</p>
      <p className="contact">此域名出售，联系邮箱：306100898@qq.com</p>
    </div>
  )
}

export default App

