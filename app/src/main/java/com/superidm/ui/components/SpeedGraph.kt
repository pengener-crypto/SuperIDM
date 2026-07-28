package com.superidm.ui.components

import androidx.compose.animation.core.*
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.*
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.*
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.unit.dp
import kotlin.math.max

/**
 * Animated real-time download speed graph.
 * @param speedHistory list of speed samples (bytes/sec) from oldest to newest
 * @param maxDataPoints max number of points to display
 * @param lineColor line color
 * @param fillColor fill gradient color (top)
 */
@Composable
fun SpeedGraph(
    speedHistory: List<Long>,
    maxDataPoints: Int = 60,
    modifier: Modifier = Modifier,
    lineColor: Color = Color(0xFF6C63FF),
    fillColor: Color = Color(0x806C63FF)
) {
    val points = remember(speedHistory, maxDataPoints) {
        if (speedHistory.size > maxDataPoints) {
            speedHistory.takeLast(maxDataPoints)
        } else {
            speedHistory
        }
    }

    val maxSpeed = points.maxOrNull() ?: 1L
    val animatedMaxSpeed by animateFloatAsState(
        targetValue = max(maxSpeed.toFloat(), 1f),
        animationSpec = tween(durationMillis = 500, easing = LinearEasing),
        label = "maxSpeedAnimation"
    )

    Canvas(modifier = modifier) {
        val width = size.width
        val height = size.height
        
        val gridLines = 4
        val gridColor = lineColor.copy(alpha = 0.1f)
        for (i in 0 until gridLines) {
            val y = height * (i.toFloat() / gridLines)
            drawLine(
                color = gridColor,
                start = Offset(0f, y),
                end = Offset(width, y),
                strokeWidth = 1.dp.toPx(),
                pathEffect = PathEffect.dashPathEffect(floatArrayOf(10f, 10f), 0f)
            )
        }

        if (points.isEmpty()) return@Canvas

        val pointWidth = if (points.size > 1) width / (maxDataPoints - 1) else width
        
        val path = Path()
        val fillPath = Path()
        
        var startX = width - (points.size - 1) * pointWidth
        if (startX < 0f) startX = 0f
        
        var prevX = startX
        var prevY = height - (points.first() / animatedMaxSpeed) * height

        path.moveTo(prevX, prevY)
        fillPath.moveTo(prevX, height)
        fillPath.lineTo(prevX, prevY)

        for (i in 1 until points.size) {
            val currentX = startX + i * pointWidth
            val currentY = height - (points[i] / animatedMaxSpeed) * height
            
            val controlX1 = (prevX + currentX) / 2
            val controlY1 = prevY
            val controlX2 = (prevX + currentX) / 2
            val controlY2 = currentY
            
            path.cubicTo(controlX1, controlY1, controlX2, controlY2, currentX, currentY)
            fillPath.cubicTo(controlX1, controlY1, controlX2, controlY2, currentX, currentY)
            
            prevX = currentX
            prevY = currentY
        }
        
        fillPath.lineTo(prevX, height)
        fillPath.close()

        drawPath(
            path = fillPath,
            brush = Brush.verticalGradient(
                colors = listOf(fillColor, Color.Transparent),
                startY = 0f,
                endY = height
            )
        )

        drawPath(
            path = path,
            color = lineColor,
            style = Stroke(width = 2.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
        )
    }
}

@Composable
fun MiniSpeedGraph(
    speedHistory: List<Long>,
    maxDataPoints: Int = 30,
    modifier: Modifier = Modifier,
    lineColor: Color = Color(0xFF6C63FF),
    fillColor: Color = Color(0x806C63FF)
) {
    val points = remember(speedHistory, maxDataPoints) {
        if (speedHistory.size > maxDataPoints) {
            speedHistory.takeLast(maxDataPoints)
        } else {
            speedHistory
        }
    }

    val maxSpeed = points.maxOrNull() ?: 1L
    val animatedMaxSpeed by animateFloatAsState(
        targetValue = max(maxSpeed.toFloat(), 1f),
        animationSpec = tween(durationMillis = 500, easing = LinearEasing),
        label = "maxSpeedAnimationMini"
    )

    Canvas(modifier = modifier.height(40.dp)) {
        val width = size.width
        val height = size.height

        if (points.isEmpty()) return@Canvas

        val pointWidth = if (points.size > 1) width / (maxDataPoints - 1) else width
        
        val path = Path()
        val fillPath = Path()
        
        var startX = width - (points.size - 1) * pointWidth
        if (startX < 0f) startX = 0f
        
        var prevX = startX
        var prevY = height - (points.first() / animatedMaxSpeed) * height

        path.moveTo(prevX, prevY)
        fillPath.moveTo(prevX, height)
        fillPath.lineTo(prevX, prevY)

        for (i in 1 until points.size) {
            val currentX = startX + i * pointWidth
            val currentY = height - (points[i] / animatedMaxSpeed) * height
            
            val controlX1 = (prevX + currentX) / 2
            val controlY1 = prevY
            val controlX2 = (prevX + currentX) / 2
            val controlY2 = currentY
            
            path.cubicTo(controlX1, controlY1, controlX2, controlY2, currentX, currentY)
            fillPath.cubicTo(controlX1, controlY1, controlX2, controlY2, currentX, currentY)
            
            prevX = currentX
            prevY = currentY
        }
        
        fillPath.lineTo(prevX, height)
        fillPath.close()

        drawPath(
            path = fillPath,
            brush = Brush.verticalGradient(
                colors = listOf(fillColor, Color.Transparent),
                startY = 0f,
                endY = height
            )
        )

        drawPath(
            path = path,
            color = lineColor,
            style = Stroke(width = 1.5.dp.toPx(), cap = StrokeCap.Round, join = StrokeJoin.Round)
        )
    }
}

fun formatSpeed(bytesPerSec: Long): String {
    return when {
        bytesPerSec >= 1_000_000_000 -> "%.1f GB/s".format(bytesPerSec / 1_000_000_000.0)
        bytesPerSec >= 1_000_000 -> "%.1f MB/s".format(bytesPerSec / 1_000_000.0)
        bytesPerSec >= 1_000 -> "%.1f KB/s".format(bytesPerSec / 1_000.0)
        else -> "$bytesPerSec B/s"
    }
}
