import React, { useState, useEffect, useCallback, useRef } from 'react';

// 游戏配置
const GRID_SIZE = 20;
const CELL_SIZE = 20;
const INITIAL_SNAKE = [{ x: 10, y: 10 }, { x: 10, y: 11 }, { x: 10, y: 12 }];
const INITIAL_DIRECTION = { x: 0, y: -1 };
const BASE_SPEED = 200;
const MIN_SPEED = 60;

type Point = { x: number; y: number };

const Game: React.FC<{ onBack: () => void }> = ({ onBack }) => {
  const [snake, setSnake] = useState<Point[]>(INITIAL_SNAKE);
  const [food, setFood] = useState<Point>({ x: 15, y: 5 });
  const [direction, setDirection] = useState<Point>(INITIAL_DIRECTION);
  const [nextDirection, setNextDirection] = useState<Point>(INITIAL_DIRECTION);
  const [gameOver, setGameOver] = useState(false);
  const [score, setScore] = useState(0);
  const [highScore, setHighScore] = useState(Number(localStorage.getItem('snakeHighScore')) || 0);
  const [isPaused, setIsPaused] = useState(false);
  
  const gameLoopRef = useRef<number | null>(null);
  const lastUpdateTimeRef = useRef<number>(0);

  // 生成随机食物，确保不在蛇身上
  const generateFood = useCallback((currentSnake: Point[]): Point => {
    let newFood;
    while (true) {
      newFood = {
        x: Math.floor(Math.random() * GRID_SIZE),
        y: Math.floor(Math.random() * GRID_SIZE),
      };
      const isOnSnake = currentSnake.some(s => s.x === newFood!.x && s.y === newFood!.y);
      if (!isOnSnake) break;
    }
    return newFood;
  }, []);

  // 重置游戏
  const resetGame = () => {
    setSnake(INITIAL_SNAKE);
    setDirection(INITIAL_DIRECTION);
    setNextDirection(INITIAL_DIRECTION);
    setFood(generateFood(INITIAL_SNAKE));
    setGameOver(false);
    setScore(0);
    setIsPaused(false);
  };

  // 难度随分数增加而增加
  const currentSpeed = Math.max(MIN_SPEED, BASE_SPEED - Math.floor(score / 5) * 15);

  // 游戏循环逻辑
  const moveSnake = useCallback(() => {
    setSnake((prevSnake) => {
      const currentDir = nextDirection;
      setDirection(currentDir);
      
      const newHead = {
        x: prevSnake[0].x + currentDir.x,
        y: prevSnake[0].y + currentDir.y,
      };

      // 检查墙壁碰撞
      if (
        newHead.x < 0 ||
        newHead.x >= GRID_SIZE ||
        newHead.y < 0 ||
        newHead.y >= GRID_SIZE
      ) {
        setGameOver(true);
        return prevSnake;
      }

      // 检查自身碰撞
      if (prevSnake.some((segment) => segment.x === newHead.x && segment.y === newHead.y)) {
        setGameOver(true);
        return prevSnake;
      }

      const newSnake = [newHead, ...prevSnake];

      // 检查是否吃到食物
      if (newHead.x === food.x && newHead.y === food.y) {
        const newScore = score + 1;
        setScore(newScore);
        if (newScore > highScore) {
          setHighScore(newScore);
          localStorage.setItem('snakeHighScore', newScore.toString());
        }
        setFood(generateFood(newSnake));
      } else {
        newSnake.pop();
      }

      return newSnake;
    });
  }, [nextDirection, food, score, highScore, generateFood]);

  // 使用 requestAnimationFrame 优化动画
  useEffect(() => {
    const update = (time: number) => {
      if (!gameOver && !isPaused) {
        const deltaTime = time - lastUpdateTimeRef.current;
        if (deltaTime >= currentSpeed) {
          moveSnake();
          lastUpdateTimeRef.current = time;
        }
      }
      gameLoopRef.current = requestAnimationFrame(update);
    };

    gameLoopRef.current = requestAnimationFrame(update);
    return () => {
      if (gameLoopRef.current) cancelAnimationFrame(gameLoopRef.current);
    };
  }, [gameOver, isPaused, currentSpeed, moveSnake]);

  // 键盘控制优化：防止 180 度转向
  useEffect(() => {
    const handleKeyPress = (e: KeyboardEvent) => {
      switch (e.key) {
        case 'ArrowUp':
          if (direction.y === 0) setNextDirection({ x: 0, y: -1 });
          break;
        case 'ArrowDown':
          if (direction.y === 0) setNextDirection({ x: 0, y: 1 });
          break;
        case 'ArrowLeft':
          if (direction.x === 0) setNextDirection({ x: -1, y: 0 });
          break;
        case 'ArrowRight':
          if (direction.x === 0) setNextDirection({ x: 1, y: 0 });
          break;
        case ' ': // 空格键暂停
          setIsPaused(prev => !prev);
          break;
      }
    };

    window.addEventListener('keydown', handleKeyPress);
    return () => window.removeEventListener('keydown', handleKeyPress);
  }, [direction]);

  // 触摸控制
  const handleDirChange = (dx: number, dy: number) => {
    if (dx !== 0 && direction.x === 0) setNextDirection({ x: dx, y: 0 });
    if (dy !== 0 && direction.y === 0) setNextDirection({ x: 0, y: dy });
  };

  return (
    <div className="container" style={{ 
      display: 'flex', 
      flexDirection: 'column', 
      alignItems: 'center',
      userSelect: 'none'
    }}>
      <h1 style={{ marginBottom: '0.5rem', fontSize: '2.5rem', textShadow: '2px 2px 4px rgba(0,0,0,0.3)' }}>
        极简贪食蛇
      </h1>
      
      <div style={{ 
        display: 'flex', 
        gap: '2rem', 
        marginBottom: '1rem',
        fontSize: '1.2rem',
        fontWeight: 'bold'
      }}>
        <span>分数: <span style={{ color: '#48bb78' }}>{score}</span></span>
        <span>最高分: <span style={{ color: '#ecc94b' }}>{highScore}</span></span>
      </div>

      <div
        style={{
          width: GRID_SIZE * CELL_SIZE,
          height: GRID_SIZE * CELL_SIZE,
          backgroundColor: '#1a202c',
          position: 'relative',
          border: '4px solid #4a5568',
          borderRadius: '8px',
          boxShadow: '0 10px 25px rgba(0,0,0,0.5)',
          overflow: 'hidden'
        }}
      >
        {/* 背景网格装饰 */}
        <div style={{
          position: 'absolute',
          width: '100%',
          height: '100%',
          backgroundImage: 'linear-gradient(#2d3748 1px, transparent 1px), linear-gradient(90deg, #2d3748 1px, transparent 1px)',
          backgroundSize: `${CELL_SIZE}px ${CELL_SIZE}px`,
          opacity: 0.1
        }} />

        {snake.map((segment, index) => (
          <div
            key={index}
            style={{
              position: 'absolute',
              left: segment.x * CELL_SIZE,
              top: segment.y * CELL_SIZE,
              width: CELL_SIZE - 1,
              height: CELL_SIZE - 1,
              backgroundColor: index === 0 ? '#48bb78' : '#38a169',
              borderRadius: index === 0 ? '4px' : '2px',
              transition: 'all 0.1s linear',
              zIndex: index === 0 ? 2 : 1,
              boxShadow: index === 0 ? '0 0 10px #48bb78' : 'none'
            }}
          >
            {/* 蛇的眼睛 */}
            {index === 0 && (
              <div style={{ position: 'relative', width: '100%', height: '100%' }}>
                <div style={{ 
                  position: 'absolute', 
                  width: '4px', 
                  height: '4px', 
                  backgroundColor: 'white', 
                  borderRadius: '50%',
                  top: '4px',
                  left: direction.x === 1 ? '12px' : '4px'
                }} />
                <div style={{ 
                  position: 'absolute', 
                  width: '4px', 
                  height: '4px', 
                  backgroundColor: 'white', 
                  borderRadius: '50%',
                  bottom: '4px',
                  left: direction.x === 1 ? '12px' : '4px'
                }} />
              </div>
            )}
          </div>
        ))}
        
        <div
          style={{
            position: 'absolute',
            left: food.x * CELL_SIZE,
            top: food.y * CELL_SIZE,
            width: CELL_SIZE - 1,
            height: CELL_SIZE - 1,
            backgroundColor: '#f56565',
            borderRadius: '50%',
            boxShadow: '0 0 15px #f56565',
            animation: 'pulse 1.5s infinite'
          }}
        />
        
        {/* 状态覆盖层 */}
        {(gameOver || isPaused) && (
          <div
            style={{
              position: 'absolute',
              top: 0,
              left: 0,
              right: 0,
              bottom: 0,
              backgroundColor: 'rgba(0, 0, 0, 0.8)',
              display: 'flex',
              flexDirection: 'column',
              justifyContent: 'center',
              alignItems: 'center',
              color: 'white',
              zIndex: 10
            }}
          >
            {gameOver ? (
              <>
                <h2 style={{ fontSize: '2rem', color: '#f56565', marginBottom: '1rem' }}>游戏结束</h2>
                <p style={{ fontSize: '1.2rem' }}>得分: {score}</p>
                <button
                  onClick={resetGame}
                  style={{
                    marginTop: '1.5rem',
                    padding: '0.8rem 2rem',
                    backgroundColor: '#48bb78',
                    color: 'white',
                    border: 'none',
                    borderRadius: '50px',
                    cursor: 'pointer',
                    fontSize: '1.1rem',
                    fontWeight: 'bold',
                    boxShadow: '0 4px 14px rgba(72, 187, 120, 0.4)'
                  }}
                >
                  再玩一次
                </button>
              </>
            ) : (
              <>
                <h2 style={{ fontSize: '2rem', marginBottom: '1rem' }}>已暂停</h2>
                <button
                  onClick={() => setIsPaused(false)}
                  style={{
                    padding: '0.8rem 2rem',
                    backgroundColor: '#4299e1',
                    color: 'white',
                    border: 'none',
                    borderRadius: '50px',
                    cursor: 'pointer',
                    fontSize: '1.1rem',
                    fontWeight: 'bold'
                  }}
                >
                  继续游戏
                </button>
              </>
            )}
          </div>
        )}
      </div>

      {/* 控制盘 (移动端/鼠标友好) */}
      <div style={{ marginTop: '2rem', display: 'grid', gridTemplateColumns: 'repeat(3, 1fr)', gap: '10px' }}>
        <div />
        <ControlButton onClick={() => handleDirChange(0, -1)}>▲</ControlButton>
        <div />
        <ControlButton onClick={() => handleDirChange(-1, 0)}>◀</ControlButton>
        <ControlButton onClick={() => setIsPaused(!isPaused)}>⏸</ControlButton>
        <ControlButton onClick={() => handleDirChange(1, 0)}>▶</ControlButton>
        <div />
        <ControlButton onClick={() => handleDirChange(0, 1)}>▼</ControlButton>
        <div />
      </div>

      <button 
        onClick={onBack}
        style={{
          marginTop: '2rem',
          padding: '0.6rem 1.5rem',
          background: 'rgba(255, 255, 255, 0.1)',
          border: '1px solid rgba(255, 255, 255, 0.3)',
          color: 'white',
          borderRadius: '8px',
          cursor: 'pointer',
          fontSize: '0.9rem'
        }}
      >
        返回首页
      </button>

      <style>{`
        @keyframes pulse {
          0% { transform: scale(1); opacity: 1; }
          50% { transform: scale(1.1); opacity: 0.8; }
          100% { transform: scale(1); opacity: 1; }
        }
      `}</style>
    </div>
  );
};

const ControlButton: React.FC<{ onClick: () => void, children: React.ReactNode }> = ({ onClick, children }) => (
  <button
    onClick={onClick}
    style={{
      width: '50px',
      height: '50px',
      backgroundColor: 'rgba(255, 255, 255, 0.1)',
      border: '1px solid rgba(255, 255, 255, 0.2)',
      color: 'white',
      borderRadius: '12px',
      cursor: 'pointer',
      fontSize: '1.2rem',
      display: 'flex',
      alignItems: 'center',
      justifyContent: 'center',
      transition: 'all 0.1s'
    }}
    onMouseDown={(e) => e.currentTarget.style.backgroundColor = 'rgba(255, 255, 255, 0.3)'}
    onMouseUp={(e) => e.currentTarget.style.backgroundColor = 'rgba(255, 255, 255, 0.1)'}
  >
    {children}
  </button>
);

export default Game;
