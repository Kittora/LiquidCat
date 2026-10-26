"""Quita los restos del fondo oscuro alrededor de los iconos.

Dentro de la silueta (contorno brillante + todo lo que encierra) los pixeles
no se tocan; fuera de ella todo se vuelve transparente.

Uso: python tools/clean_theme_icons.py <carpeta con los PNG originales> assets/theme-icons 256
Requiere Pillow.
"""
import sys
from pathlib import Path

from PIL import Image, ImageChops, ImageDraw, ImageFilter

SRC, DST = Path(sys.argv[1]), Path(sys.argv[2])
SIZE = int(sys.argv[3]) if len(sys.argv) > 3 else 256
DST.mkdir(parents=True, exist_ok=True)

NAMES = {
    "01_catppuccin_mocha": "catppuccin",
    "02_catppuccin_macchiato": "catppuccin-macchiato",
    "03_catppuccin_frappe": "catppuccin-frappe",
    "04_catppuccin_latte": "catppuccin-latte",
    "05_dracula": "dracula",
    "06_amoled": "amoled",
    "07_dark": "dark",
    "08_light": "light",
    "09_fluent": "fluent",
    "10_gruvbox": "gruvbox",
    "11_nord": "nord",
    "12_tokyo_night": "tokyo-night",
    "13_neo_brutalist_light": "neo-brutalist-light",
    "14_neo_brutalist_dark": "neo-brutalist-dark",
}


def silhouette(im: Image.Image) -> Image.Image:
    """Mascara del contorno brillante con su interior relleno."""
    r, g, b, a = im.split()
    value = ImageChops.lighter(ImageChops.lighter(r, g), b)
    bright = Image.eval(value, lambda v: 255 if v > 110 else 0)
    bright = ImageChops.multiply(bright, Image.eval(a, lambda v: 255 if v > 128 else 0))
    # cierre morfologico: une huecos pequenos del contorno sin engordarlo
    closed = bright.filter(ImageFilter.MaxFilter(9)).filter(ImageFilter.MinFilter(9))
    # relleno: todo lo que no se alcanza desde la esquina es interior
    flood = closed.copy()
    ImageDraw.floodfill(flood, (0, 0), 128)
    interior = Image.eval(flood, lambda v: 0 if v == 128 else 255)
    # se encoge unos px desde el borde exterior: el anillo oscuro pegado al
    # contorno (resto del fondo) queda fuera y se limpia; el contorno brillante
    # se conserva igual porque se suma aparte.
    interior = interior.filter(ImageFilter.MinFilter(13))
    return ImageChops.lighter(interior, bright)


def defringe(im: Image.Image) -> Image.Image:
    im = im.convert("RGBA")
    # Fuera de la silueta no hay nada que conservar (solo restos del fondo):
    # se vuelve transparente, con el borde de la mascara suavizado para que
    # el contorno no quede recortado a sierra.
    # 2 px menos de borde: los pixeles justo en la orilla del contorno estan
    # mezclados con el fondo oscuro y dejaban una linea gris punteada.
    hard = silhouette(im).filter(ImageFilter.MinFilter(5))
    # el suavizado solo puede quitar alfa (min con la mascara dura): si se
    # extendiera hacia fuera volveria a meter los puntitos pegados al borde.
    mask = ImageChops.darker(hard, hard.filter(ImageFilter.GaussianBlur(1.6)))
    r, g, b, a = im.split()
    a = ImageChops.multiply(a, mask)
    return bleed(Image.merge("RGBA", (r, g, b, a)))


def bleed(im: Image.Image) -> Image.Image:
    """Da a los pixeles transparentes el color de sus vecinos visibles: al
    reducir la imagen sin alfa premultiplicado no aparece un halo negro."""
    r, g, b, a = im.split()
    pre = [ImageChops.multiply(c, a) for c in (r, g, b)]
    blur = ImageFilter.GaussianBlur(12)
    pre_b = [c.filter(blur) for c in pre]
    a_b = a.filter(blur)
    px = im.load()
    pb = [c.load() for c in pre_b]
    ab = a_b.load()
    w, h = im.size
    for y in range(h):
        for x in range(w):
            if px[x, y][3] == 0:
                k = ab[x, y]
                if k:
                    px[x, y] = tuple(min(255, round(c[x, y] * 255 / k)) for c in pb) + (0,)
    return im


# Encuadre comun (sobre un lienzo de 1024): todos los gatos con el mismo alto
# de cuerpo, en la misma x y apoyados en la misma linea de abajo. Las
# estrellas y los puntos acompanan al gato con su posicion relativa. El gato
# va algo a la izquierda del centro para que el conjunto (con los puntos de la
# derecha) quede centrado y con el mismo margen a ambos lados.
BODY_H = 740
BODY_CX = 470
BODY_BOTTOM = 962


def normalize(im: Image.Image) -> Image.Image:
    a = im.getchannel("A").point(lambda v: 255 if v > 60 else 0)
    # el cuerpo es la mancha conectada que contiene el centro de la cara
    ImageDraw.floodfill(a, (480, 560), 128)
    body = a.point(lambda v: 255 if v == 128 else 0).getbbox()
    k = BODY_H / (body[3] - body[1])
    scaled = im.resize((round(im.width * k), round(im.height * k)), Image.LANCZOS)
    dx = round(BODY_CX - (body[0] + body[2]) / 2 * k)
    dy = round(BODY_BOTTOM - body[3] * k)
    canvas = Image.new("RGBA", im.size, (0, 0, 0, 0))
    # paste sin mascara: copia RGBA tal cual (con mascara el alfa se multiplicaria)
    canvas.paste(scaled, (dx, dy))
    return canvas


for f in sorted(SRC.glob("*.png")):
    name = NAMES.get(f.stem)
    if not name:
        continue
    img = normalize(defringe(Image.open(f)))
    img = img.resize((SIZE, SIZE), Image.LANCZOS)
    img.save(DST / f"{name}.png", optimize=True)
    print(name)
