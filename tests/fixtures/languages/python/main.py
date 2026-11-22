import os


def here() -> str:
    return os.path.join(os.getcwd(), "data")
