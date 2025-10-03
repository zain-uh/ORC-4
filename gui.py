from pathlib import Path

# from tkinter import *
# Explicit imports to satisfy Flake8
from tkinter import Tk, Canvas, Entry, Text, Button, PhotoImage
import youtube

OUTPUT_PATH = Path(__file__).parent
ASSETS_PATH = OUTPUT_PATH / Path(r"assets\frame0")


def relative_to_assets(path: str) -> Path:
    return ASSETS_PATH / Path(path)


window = Tk()

window.geometry("960x540")
window.configure(bg = "#FFFFFF")

def open_youtube():
    youtube.start(window)

canvas = Canvas(
    window,
    bg = "#FFFFFF",
    height = 540,
    width = 960,
    bd = 0,
    highlightthickness = 0,
    relief = "ridge"
)


canvas.place(x = 0, y = 0)
image_image_1 = PhotoImage(
    file=relative_to_assets("image_1.png"))
image_1 = canvas.create_image(
    495.0,
    270.0,
    image=image_image_1
)

image_image_2 = PhotoImage(
    file=relative_to_assets("image_2.png"))
image_2 = canvas.create_image(
    951.0,
    270.0,
    image=image_image_2
)

canvas.create_text(
    863.0,
    9.0,
    anchor="nw",
    text="100%",
    fill="#282828",
    font=("IBM Plex Mono SemiBold", 32 * -1)
)
window.resizable(False, False)
window.mainloop()
