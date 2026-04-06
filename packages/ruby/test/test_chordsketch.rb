# frozen_string_literal: true

require "minitest/autorun"
require "chordsketch"

class TestChordSketch < Minitest::Test
  MINIMAL_INPUT = "{title: Test}\n[C]Hello"

  def test_version
    v = Chordsketch.version
    refute_empty v
  end

  def test_render_text
    text = Chordsketch.parse_and_render_text(MINIMAL_INPUT, nil, nil)
    assert_includes text, "Test"
    assert_includes text, "Hello"
  end

  def test_render_html
    html = Chordsketch.parse_and_render_html(MINIMAL_INPUT, nil, nil)
    assert_includes html, "Test"
  end

  def test_render_pdf
    pdf = Chordsketch.parse_and_render_pdf(MINIMAL_INPUT, nil, nil)
    refute_empty pdf
    assert_equal "%PDF", pdf[0..3]
  end

  def test_validate
    errors = Chordsketch.validate(MINIMAL_INPUT)
    assert_kind_of Array, errors
    assert_empty errors
  end

  def test_render_with_preset
    text = Chordsketch.parse_and_render_text(MINIMAL_INPUT, "guitar", nil)
    assert_includes text, "Test"
  end

  def test_render_with_transpose
    text = Chordsketch.parse_and_render_text(MINIMAL_INPUT, nil, 2)
    refute_empty text
  end
end
