# Python reference for task 45 (headless Pong core). Same rules, same integer arithmetic.
from dataclasses import dataclass, replace

WIDTH, HEIGHT, PADDLE, WIN_SCORE, MAX_TICKS = 80, 24, 5, 3, 5000


@dataclass(frozen=True)
class Ball:
    x: int
    y: int
    dx: int
    dy: int


@dataclass(frozen=True)
class Game:
    ball: Ball
    left: int
    right: int
    left_score: int
    right_score: int
    hits: int
    tick: int


def clamp_paddle(top):
    return max(0, min(top, HEIGHT - PADDLE))


def follow(top, target):
    center = top + PADDLE // 2
    if target < center:
        return clamp_paddle(top - 1)
    if target > center:
        return clamp_paddle(top + 1)
    return top


def serve(points, toward_right):
    return Ball(WIDTH // 2, HEIGHT // 2, 2 if toward_right else -2, 1 if points % 2 == 0 else -1)


def covers(top, y):
    return top <= y < top + PADDLE


def spin(top, y, dy):
    offset = y - (top + PADDLE // 2)
    return -dy if offset == 0 else offset


def step(g):
    g = replace(g, tick=g.tick + 1)
    if g.ball.dx < 0 and g.tick % 5 != 0:
        g = replace(g, left=follow(g.left, g.ball.y))
    if g.ball.dx > 0:
        g = replace(g, right=follow(g.right, g.ball.y))
    b = g.ball
    x, y, dx, dy = b.x + b.dx, b.y + b.dy, b.dx, b.dy
    if y < 0:
        y, dy = -y, -dy
    if y > HEIGHT - 1:
        y, dy = 2 * (HEIGHT - 1) - y, -dy
    if x <= 0:
        if covers(g.left, y):
            return replace(g, ball=Ball(-x, y, -dx, spin(g.left, y, dy)), hits=g.hits + 1)
        g = replace(g, right_score=g.right_score + 1)
        return replace(g, ball=serve(g.left_score + g.right_score, False))
    if x >= WIDTH:
        if covers(g.right, y):
            return replace(g, ball=Ball(2 * WIDTH - x, y, -dx, spin(g.right, y, dy)), hits=g.hits + 1)
        g = replace(g, left_score=g.left_score + 1)
        return replace(g, ball=serve(g.left_score + g.right_score, True))
    return replace(g, ball=Ball(x, y, dx, dy))


start = HEIGHT // 2 - PADDLE // 2
game = Game(serve(0, True), start, start, 0, 0, 0, 0)
while game.left_score < WIN_SCORE and game.right_score < WIN_SCORE and game.tick < MAX_TICKS:
    before = game.left_score + game.right_score
    game = step(game)
    if game.left_score + game.right_score > before:
        print(f"tick {game.tick}: left {game.left_score} - right {game.right_score}")
winner = "left" if game.left_score > game.right_score else "right"
print(f"{winner} wins after {game.tick} ticks and {game.hits} paddle hits")
