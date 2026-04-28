from backend.app.core.sorter import (
    calculate_padding,
    clean_title,
    generate_filename,
    intro_filename,
    parse_episode_number,
    sanitize_filename_part,
    sorted_for_filesystem,
)


def test_padding_keeps_story_machine_order_stable():
    assert calculate_padding(9) == 3
    assert calculate_padding(52) == 3
    assert calculate_padding(100) == 3
    assert calculate_padding(1000) == 4


def test_episode_number_patterns():
    assert parse_episode_number("萌鸡小队.S01E02.找妈妈.mp4", 9) == 2
    assert parse_episode_number("第12集.泥坑.mp4", 9) == 12
    assert parse_episode_number("03 小星星.mp4", 9) == 3
    assert parse_episode_number("小兔子乖乖.mp4", 9) == 9


def test_clean_title_removes_collection_episode_and_tech_tags():
    assert clean_title("萌鸡小队.S01E02.找妈妈.1080p.WEB-DL.x264.mp4", "萌鸡小队") == "找妈妈"
    assert clean_title("第01集.泥坑.mp4", "小猪佩奇第三季") == "泥坑"


def test_filename_generation_and_intro():
    assert generate_filename(2, "找/妈妈?", "mp3", 3) == "002_找妈妈？.mp3"
    assert intro_filename("萌鸡小队第一季", "mp3") == "000_萌鸡小队第一季.mp3"
    assert sanitize_filename_part('a:b*c?"d<e>f|') == "a：bc？def"


def test_filesystem_sorting_modes_are_explicit():
    names = ["10_故事.mp3", "2_故事.mp3", "001_片头.mp3"]

    assert sorted_for_filesystem(names, "ntfs") == ["001_片头.mp3", "10_故事.mp3", "2_故事.mp3"]
    assert sorted_for_filesystem(names, "natural") == ["001_片头.mp3", "2_故事.mp3", "10_故事.mp3"]
    assert calculate_padding(12, "4") == 4
